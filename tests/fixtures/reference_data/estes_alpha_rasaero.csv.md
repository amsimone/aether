# Estes Alpha — RASAero reference data

This directory will contain CSV exports from RASAero for the Estes Alpha
validation rocket. The file `estes_alpha_rasaero.csv` will have columns:

- `mach` — Mach number
- `cd` — drag coefficient
- `cn_alpha` — normal force slope (per radian)
- `cp_calibers` — center of pressure in calibers from the nose tip

These are the values our aero module must match within the tolerances
specified in `fixtures/rockets/estes_alpha.json`.

**TODO**: Export the data from RASAero and place the CSV here. RASAero
is a Windows-only tool; the export must be done on a Windows machine or
via Wine.

Until this file exists, the `validation::aero::estnes_alpha_cd_vs_mach`
test remains `#[ignore]`.
