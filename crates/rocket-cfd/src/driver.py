"""Aether OpenFOAM v2606 / Gmsh CFD adapter, schema 1.
CAD +x is aft; flight-body axes transform by diag(-1, 1, -1).
Generated jobs contain data, never executable commands from imported files.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import traceback


def header(name, kind="dictionary"):
    return f"FoamFile {{ version 2.0; format ascii; class {kind}; object {name}; }}\n"


def write_dictionary(root, name, text):
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(header(path.name) + text)


def cancelled(root):
    if (root / "cancel.flag").exists():
        raise InterruptedError("CFD cancelled; partial artifacts retained")


def build_mesh(root, data):
    import gmsh
    gmsh.initialize()
    try:
        gmsh.option.setNumber("General.Terminal", 1)
        gmsh.model.add("vehicle_external_flow")
        occ = gmsh.model.occ
        geometry = data["geometry"]
        solids = []
        for profile in geometry["profiles"]:
            xy = profile["points"]
            outline = [occ.addPoint(x, r, 0) for x, r in xy]
            # The centerline closes each solid; contiguous solids are fused by OCC.
            curves = [occ.addSpline(outline)] if len(outline) > 2 else [occ.addLine(*outline)]
            end = occ.addPoint(xy[-1][0], 0, 0)
            curves.append(occ.addLine(outline[-1], end))
            start = outline[0] if abs(xy[0][1]) < 1e-14 else occ.addPoint(xy[0][0], 0, 0)
            curves.append(occ.addLine(end, start))
            if start != outline[0]:
                curves.append(occ.addLine(start, outline[0]))
            face = occ.addPlaneSurface([occ.addCurveLoop(curves)])
            solids.extend(tag for dim, tag in occ.revolve([(2, face)], 0, 0, 0, 1, 0, 0, 2 * math.pi) if dim == 3)
        for fin in geometry["fins"]:
            for i in range(fin["count"]):
                points = [occ.addPoint(x, y, -fin["thickness"] / 2) for x, y in fin["polygon"]]
                curves = [occ.addLine(points[j], points[(j + 1) % len(points)]) for j in range(len(points))]
                face = occ.addPlaneSurface([occ.addCurveLoop(curves)])
                volumes = [(d, t) for d, t in occ.extrude([(2, face)], 0, 0, fin["thickness"]) if d == 3]
                occ.rotate(volumes, 0, 0, 0, 1, 0, 0, 2 * math.pi * i / fin["count"])
                solids.extend(t for _, t in volumes)
        for lug in geometry["lugs"]:
            outer = occ.addCylinder(lug["x"], lug["offset"], 0, lug["length"], 0, 0, lug["outer"])
            if lug["inner"] > 0:
                inner = occ.addCylinder(lug["x"] - 1e-6, lug["offset"], 0, lug["length"] + 2e-6, 0, 0, lug["inner"])
                volumes, _ = occ.cut([(3, outer)], [(3, inner)])
                solids.extend(t for d, t in volumes if d == 3)
            else:
                solids.append(outer)
        for pod in geometry["pods"]:
            solids.append(occ.addCylinder(pod["x"], pod["offset"] * math.cos(pod["angle"]), pod["offset"] * math.sin(pod["angle"]), pod["length"], 0, 0, pod["radius"]))
        body, _ = occ.fuse([(3, solids[0])], [(3, tag) for tag in solids[1:]]) if len(solids) > 1 else ([(3, solids[0])], [])
        occ.synchronize()
        gmsh.write(str(root / "vehicle.brep"))
        length = geometry["length"]
        bounds = (-5 * length, -5 * length, -5 * length, 11 * length, 5 * length, 5 * length)
        box = occ.addBox(bounds[0], bounds[1], bounds[2], bounds[3] - bounds[0], 10 * length, 10 * length)
        fluid, _ = occ.cut([(3, box)], body)
        occ.synchronize()
        if len(fluid) != 1:
            raise ValueError("CFD geometry must produce one connected external fluid volume")
        wall, far = [], []
        for dim, tag in gmsh.model.getBoundary(fluid, oriented=False):
            if dim != 2:
                continue
            bb = gmsh.model.getBoundingBox(dim, tag)
            outside = any(abs(bb[i] - bounds[i]) < 1e-6 and abs(bb[i + 3] - bounds[i]) < 1e-6 for i in range(3))
            outside |= any(abs(bb[i] - bounds[i + 3]) < 1e-6 and abs(bb[i + 3] - bounds[i + 3]) < 1e-6 for i in range(3))
            (far if outside else wall).append(tag)
        if not wall or not far:
            raise ValueError("Failed to identify vehicle and freestream boundary surfaces")
        gmsh.model.addPhysicalGroup(2, wall, name="wall")
        gmsh.model.addPhysicalGroup(2, far, name="freestream")
        gmsh.model.addPhysicalGroup(3, [t for d, t in fluid], name="fluid")
        distance = gmsh.model.mesh.field.add("Distance")
        gmsh.model.mesh.field.setNumbers(distance, "SurfacesList", wall)
        gmsh.model.mesh.field.setNumber(distance, "Sampling", 50)
        threshold = gmsh.model.mesh.field.add("Threshold")
        gmsh.model.mesh.field.setNumber(threshold, "InField", distance)
        near = data["settings"]["mesh_size_m"]
        for key, value in {"SizeMin": near, "SizeMax": length / 2, "DistMin": geometry["diameter"], "DistMax": length}.items():
            gmsh.model.mesh.field.setNumber(threshold, key, value)
        gmsh.model.mesh.field.setAsBackgroundMesh(threshold)
        gmsh.option.setNumber("Mesh.MeshSizeFromPoints", 0)
        gmsh.option.setNumber("Mesh.MeshSizeExtendFromBoundary", 0)
        gmsh.option.setNumber("Mesh.MeshSizeFromCurvature", 20)
        gmsh.option.setNumber("Mesh.MeshSizeMin", near / 5)
        gmsh.option.setNumber("Mesh.MeshSizeMax", length / 2)
        gmsh.option.setNumber("Mesh.Algorithm3D", 10)
        gmsh.option.setNumber("Mesh.OptimizeThreshold", 0.5)
        gmsh.option.setNumber("Mesh.MshFileVersion", 2.2)
        gmsh.option.setNumber("General.NumThreads", 2)
        cancelled(root)
        gmsh.model.mesh.generate(3)
        gmsh.model.mesh.optimize("Relocate3D", niter=5)
        gmsh.model.mesh.optimize("Netgen", niter=5)
        gmsh.write(str(root / "fluid.msh"))
        cancelled(root)
        return gmsh.__version__
    finally:
        gmsh.finalize()


def case_files(root, settings):
    velocity = settings["mach"] * math.sqrt(1.4 * 287.05 * settings["temperature_k"])
    alpha, beta = math.radians(settings["alpha_deg"]), math.radians(settings["beta_deg"])
    u = f"({velocity * math.cos(alpha) * math.cos(beta)} {-velocity * math.sin(beta)} {velocity * math.sin(alpha) * math.cos(beta)})"
    density = settings["pressure_pa"] / (287.05 * settings["temperature_k"])
    viscosity = 1.716e-5 * (settings["temperature_k"] / 273.15) ** 1.5 * (273.15 + 110.4) / (settings["temperature_k"] + 110.4)
    k = 1.5 * (velocity * settings["turbulence_intensity"]) ** 2
    omega = math.sqrt(k) / (0.09 ** 0.25 * 0.07 * settings["reference_length_m"])
    def field(name, dimensions, value, wall, far, vector=False):
        path = root / "0" / name
        path.parent.mkdir(exist_ok=True)
        path.write_text(header(name, "volVectorField" if vector else "volScalarField") + f"dimensions {dimensions};\ninternalField uniform {value};\nboundaryField {{ wall {{ {wall} }} freestream {{ {far} }} }}\n")
    field("U", "[0 1 -1 0 0 0 0]", u, "type noSlip;", f"type freestreamVelocity; freestreamValue uniform {u}; value uniform {u};", True)
    field("p", "[1 -1 -2 0 0 0 0]", settings["pressure_pa"], "type zeroGradient;", f"type freestreamPressure; freestreamValue uniform {settings['pressure_pa']};")
    field("T", "[0 0 0 1 0 0 0]", settings["temperature_k"], "type zeroGradient;", f"type inletOutlet; inletValue uniform {settings['temperature_k']}; value uniform {settings['temperature_k']};")
    field("k", "[0 2 -2 0 0 0 0]", k, f"type kqRWallFunction; value uniform {k};", f"type inletOutlet; inletValue uniform {k}; value uniform {k};")
    field("omega", "[0 0 -1 0 0 0 0]", omega, f"type omegaWallFunction; value uniform {omega};", f"type inletOutlet; inletValue uniform {omega}; value uniform {omega};")
    field("nut", "[0 2 -1 0 0 0 0]", 0, "type nutkWallFunction; value uniform 0;", "type calculated; value uniform 0;")
    field("alphat", "[1 -1 -1 0 0 0 0]", 0, "type compressible::alphatWallFunction; Prt 0.85; value uniform 0;", "type calculated; value uniform 0;")
    write_dictionary(root, "constant/thermophysicalProperties", f"""thermoType {{ type hePsiThermo; mixture pureMixture; transport const; thermo hConst; equationOfState perfectGas; specie specie; energy sensibleInternalEnergy; }}
mixture {{ specie {{ molWeight 28.966; }} thermodynamics {{ Cp 1004.675; Hf 0; }} transport {{ mu {viscosity}; Pr 0.71; }} }}
""")
    write_dictionary(root, "constant/turbulenceProperties", "simulationType RAS; RAS { RASModel kOmegaSST; turbulence on; printCoeffs on; }\n")
    write_dictionary(root, "system/fvSchemes", """ddtSchemes { default steadyState; }
gradSchemes { default cellLimited Gauss linear 1; }
divSchemes { default none; div(phi,U) bounded Gauss upwind; div(phi,e) bounded Gauss upwind; div(phi,K) bounded Gauss upwind; div(phi,Ekp) bounded Gauss upwind; div(phi,k) bounded Gauss upwind; div(phi,omega) bounded Gauss upwind; div(phid,p) Gauss upwind; div((phi|interpolate(rho)),p) bounded Gauss upwind; div(((rho*nuEff)*dev2(T(grad(U))))) Gauss linear; }
laplacianSchemes { default Gauss linear limited 0.5; } interpolationSchemes { default linear; } snGradSchemes { default limited 0.5; } wallDist { method meshWave; }
""")
    write_dictionary(root, "system/fvSolution", """solvers { p { solver GAMG; smoother GaussSeidel; tolerance 1e-7; relTol 0.01; } "(U|k|omega|e)" { solver PBiCGStab; preconditioner DILU; tolerance 1e-7; relTol 0.1; } }
SIMPLE { nNonOrthogonalCorrectors 1; pMinFactor 0.1; pMaxFactor 2; residualControl { p 1e-5; U 1e-5; "(k|omega|e)" 1e-5; } }
relaxationFactors { fields { p 0.3; rho 0.05; } equations { U 0.3; e 0.5; "(k|omega)" 0.5; } }
""")
    write_dictionary(root, "system/controlDict", f"""application rhoSimpleFoam; startFrom startTime; startTime 0; stopAt endTime; endTime {settings['iterations']}; deltaT 1;
writeControl timeStep; writeInterval {settings['iterations']}; purgeWrite 1; writeFormat ascii; writePrecision 12; runTimeModifiable false;
functions {{ coefficients {{ type forceCoeffs; libs (forces); writeControl timeStep; writeInterval 1; log false; patches (wall); rho rho; rhoInf {density};
dragDir (1 0 0); liftDir (0 0 1); pitchAxis (0 1 0); CofR ({settings['moment_reference_station_m']} 0 0); magUInf {velocity}; lRef {settings['reference_length_m']}; Aref {settings['reference_area_m2']}; }} }}
""")
    return velocity * density * settings["reference_length_m"] / viscosity


def foam_command(root, bashrc, program, *args):
    cancelled(root)
    # Paths and arguments occupy positional parameters; no imported text is shell code.
    argv = ["/bin/bash", "-c", 'aether_foam_bashrc=$1; shift; aether_foam_command=("$@"); set --; source "$aether_foam_bashrc" >/dev/null || exit $?; exec "${aether_foam_command[@]}"', "aether-cfd", str(bashrc), program, *map(str, args)]
    with (root / f"{program}.log").open("w") as log:
        child = subprocess.Popen(argv, cwd=root, stdout=log, stderr=subprocess.STDOUT)
        while child.poll() is None:
            if (root / "cancel.flag").exists():
                child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
                raise InterruptedError("CFD cancelled")
            time.sleep(0.1)
        if child.returncode:
            raise RuntimeError(f"{program} exited {child.returncode}; see {program}.log")
    return (root / f"{program}.log").read_text(errors="replace")


def coefficients(root):
    files = list((root / "postProcessing" / "coefficients").glob("*/coefficient.dat"))
    if not files:
        raise RuntimeError("Solver produced no coefficient history")
    lines = max(files, key=lambda p: p.stat().st_mtime).read_text().splitlines()
    header_line = next((line for line in lines if line.startswith("# Time")), None)
    if header_line is None:
        raise RuntimeError("Unrecognized forceCoeffs output header")
    names = header_line[1:].split()
    rows = []
    for line in lines:
        if line and not line.startswith("#"):
            values = list(map(float, line.split()))
            if len(values) == len(names):
                rows.append(dict(zip(names, values)))
    if not rows or not all(math.isfinite(v) for row in rows for v in row.values()):
        raise RuntimeError("No finite solver coefficient samples")
    required = ["Cd", "Cs", "Cl", "CmRoll", "CmPitch", "CmYaw"]
    if not all(name in rows[-1] for name in required):
        raise RuntimeError("Required body force/moment columns missing")
    window = rows[-40:]
    drift = max((max(row[name] for row in window) - min(row[name] for row in window)) / max(abs(sum(row[name] for row in window) / len(window)), 0.01) for name in required)
    last = rows[-1]
    # CAD axes to right-handed flight body axes: diag(-1,+1,-1).
    row = dict(ca=last["Cd"], cy=last["Cs"], cn=-last["Cl"], cl=-last["CmRoll"], cm=last["CmPitch"], cn_yaw=-last["CmYaw"])
    return row, int(last["Time"]), drift, len(window)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--foam-bashrc", required=True, type=Path)
    args = parser.parse_args()
    root = args.case.resolve()
    data = json.loads((root / "input.json").read_text())
    if data["version"] != 1 or data["driver_sha256"] != hashlib.sha256(Path(__file__).read_bytes()).hexdigest():
        raise ValueError("Study driver checksum or schema mismatch")
    if (root / "fluid.msh").exists() or (root / "constant").exists():
        raise ValueError("Study already started; export a fresh job to rerun")
    gmsh_version = build_mesh(root, data)
    reynolds = case_files(root, data["settings"])
    foam_command(root, args.foam_bashrc, "gmshToFoam", "fluid.msh")
    boundary = root / "constant" / "polyMesh" / "boundary"
    text, count = re.subn(r"(\bwall\s*\{\s*type\s+)patch;", r"\1wall;", boundary.read_text())
    if count != 1:
        raise ValueError("Expected one vehicle-wall boundary from the mesh")
    boundary.write_text(text)
    check = foam_command(root, args.foam_bashrc, "checkMesh", "-allTopology", "-allGeometry")
    if "Mesh OK." not in check:
        raise ValueError("Mesh quality checks failed; inspect checkMesh.log")
    solver = foam_command(root, args.foam_bashrc, "rhoSimpleFoam")
    row, iteration, drift, window = coefficients(root)
    settings = data["settings"]
    row.update(mach=settings["mach"], alpha_rad=math.radians(settings["alpha_deg"]), beta_rad=math.radians(settings["beta_deg"]))
    converged = "SIMPLE solution converged" in solver and window >= 40 and drift < 0.01 and row["ca"] >= 0
    cells = int(re.search(r"cells:\s+(\d+)", check).group(1))
    version = re.search(r"\bversion=(\S+)", solver)
    solver_version = "OpenFOAM " + version.group(1) if version else "OpenFOAM (see solver log)"
    outcome = dict(version=1, vehicle_sha256=data["vehicle_sha256"], settings=settings, coefficients=row, reynolds=reynolds, mesh_cells=cells, iterations=iteration, converged=converged, coefficient_relative_change=drift, solver_version=solver_version, gmsh_version=gmsh_version, dataset=None)
    if converged:
        metadata = dict(name="OpenFOAM steady RANS point", source=f"Aether CFD adapter v1 / {solver_version} / Gmsh {gmsh_version}", vehicle_sha256=data["vehicle_sha256"], axes="body_x_nose_y_right_z_rh", reference_area_m2=settings["reference_area_m2"], reference_length_m=settings["reference_length_m"], moment_reference_station_m=settings["moment_reference_station_m"], reynolds_min=reynolds * (1 - 1e-8), reynolds_max=reynolds * (1 + 1e-8), evidence="Excluded geometry: " + "; ".join(data["geometry"]["omitted"]) + ". Iterative convergence and coefficient stability passed on one tetrahedral mesh. kOmegaSST, wall functions, first-order upwind. No boundary-layer inflation, mesh/domain convergence study or experimental validation. Static point only; no dynamic derivatives. See input.json, vehicle.brep, fluid.msh, checkMesh.log and rhoSimpleFoam.log.")
        outcome["dataset"] = dict(version=1, metadata=metadata, rows=[row])
        (root / "aerodynamics.json").write_text(json.dumps(outcome["dataset"], indent=2, allow_nan=False))
    (root / "result.json").write_text(json.dumps(outcome, indent=2, allow_nan=False))
    (root / "view.foam").touch()
    manifest = dict(schema=1, status="converged_unvalidated" if converged else "not_converged", files={str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(root.rglob("*")) if p.is_file() and p.name not in ("manifest.json", "driver.log")})
    (root / "manifest.json").write_text(json.dumps(manifest, indent=2))
    print(json.dumps(dict(status=manifest["status"], cells=cells, iteration=iteration, coefficients=row)))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        traceback.print_exc()
        # Failed jobs retain their exact input and stage logs, with an explicit outcome.
        try:
            case = Path(sys.argv[sys.argv.index("--case") + 1])
            (case / "failure.json").write_text(json.dumps(dict(status="cancelled" if isinstance(error, InterruptedError) else "failed", error=str(error)), indent=2))
        finally:
            sys.exit(130 if isinstance(error, InterruptedError) else 1)
