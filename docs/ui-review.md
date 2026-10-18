# Editor UI review

## Findings addressed

- **Invisible geometry:** the view matrix had an incorrect basis entry and the
  projection used the opposite handedness. Surface winding also faced inward
  on noses, cylinders, transitions, and two faces of the mass proxy. Corrected
  the matrices and winding, added depth testing, and selected wgpu explicitly.
- **Selection stopped at the inspector:** component IDs now survive mesh
  generation. Selecting a row highlights the corresponding geometry, and clicking
  visible triangles selects the closest part. Isolate mode reveals internal
  components and fits the camera when switching parts.
- **Poor use of space:** replaced the fixed 500 × 400 viewport and single crowded
  sidebar with a resizable parts list, full canvas, and separate inspector.
  Added a consistent dark theme, quieter component colors, compact dimension
  inputs, camera instructions, a Fit view action, and an empty-design action.
  Advanced mass/aero details are collapsed by default.
- **Incomplete starter:** added a reusable, editable 250 mm Aether One design
  with three tail fins, launch lug, recovery mass, and an 18 mm motor mount
  specification. It opens by default and can be restored from the File menu or
  parts panel. Its JSON lives in `examples/aether-one.rocket` and uses the
  normal save/load schema.
- **Edits that failed silently:** nose-shape changes now persist; geometry
  comparison includes all parameters rather than only IDs, positions, and lengths.
  New fins sit at the body's tail and use their referenced mounting tube radius.
  Launch lugs sit outside the body; pods honor radial offsets; mass proxies are
  translated once. Loaded component IDs cannot collide with subsequently added
  components in normal use.
- **Other interaction failures:** Delete/Backspace and Duplicate shortcuts avoid
  focused text entry. Save on a new design opens Save As. Loaded designs remain
  clean, file failures appear in the app, and design changes clear old simulation
  results. Motor curves are embedded so reference simulations work outside the
  repository. Simulation controls stack within the inspector width.

## Verification

Automated regressions cover camera centering and clip-space visibility at three
aspect ratios, visible-part picking and background misses, outward normals,
recovery-mass positioning, reference design stability and JSON round-trip, and an
end-to-end simulation with a bundled motor. The workspace's existing tests are
also run; pre-existing ignored validation cases remain ignored.

Manual native-app checks cover the layout, model visibility, selection and
isolation, nose-shape editing, and running the reference simulation.

## Remaining UX work

- Add undo/redo and unsaved-change protection before replacing a design.
- Explain or repair mount references when deleting a body tube.
- Add a named component system, design renaming, and a clear loaded-motor versus
  dry-mass presentation.
- Move longer simulations off the UI thread and provide cancellation.
- Add validated assembly constraints and richer design diagnostics. The reference
  model is intended for software testing, not as a certified flight design.
