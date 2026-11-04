# CellScript NovaSeal certification

This optional native package owns NovaSeal evidence reconstruction and summary
policy. It imports the producer canonical-JSON package, with no compiler or
independent-checker dependency. Default compiler and browser builds do not
include this package.

`build_report(checkout_root)` reads the explicitly supplied checkout and its
versioned proposal, verifier and evidence paths. The package does not ship those
production inputs or substitute bundled unit data for missing evidence. A
missing or invalid input retains the existing failure/status semantics.

Package unit tests use bounded inputs under `tests/resources/` and temporary
repositories. They do not walk from the installed package directory to an
assumed parent workspace. The resource index binds each copied input's exact
bytes and SHA-256. The compiler checkout's `certification_resources` integration
test compares this closed inventory with the actual proposal inputs; intentional
resource changes need a reviewed update to the data and index.

These files are unit-test data. Their presence, successful archive tests and
profile documentation do not provide production evidence or certification.
