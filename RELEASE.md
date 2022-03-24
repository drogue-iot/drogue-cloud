# Release cheat sheet

## Next version

Preparing for a new version (not release, like a milestone):

* Change the version in all crates to e.g. `0.4.0`
  * Pay attention to the `service-api` crate as its version will be reported externally
  * Pay attention to the open API spec as it does not pull its version from Cargo. See `console-backend/api/index.yaml` file.

## Overall process

* Get rid of as many as possible "needs release" patches in `Cargo.toml` and `console-frontend/Cargo.toml`
* Create a new tag
  * Start with a `v0.x.0-rc1` version
  * The final version should be `v0.x.0`
* Push the tag
* Wait for the build
* Test the instructions in the following "Installation" subsections
* For each installation:
  * Test the links on the command line
  * Test the links in the web console
  * Try out the example commands
* Create a branch `release-0.x`
  * Ensure to switch the doc version to 0.x too: `docs/antora.yml`
* Release the [helm charts](https://github.com/drogue-iot/drogue-cloud-helm-charts):
  * Create a `release-0.x` branch
  * Updates the charts `version` fields in `Chart.yaml` (each chart must be updated).
  * Note : watch out for the dependencies fields. The ^ tends to break things.
  * You can look at a previous release [example](https://github.com/drogue-iot/drogue-cloud-helm-charts/commit/120d178eea2728a09247ecee2028e5061f6392c5)
  * GH Actions will see the new branch and will release the charts.

## Release text

The text that goes into the final GitHub release record comes from `installer/README.md`
