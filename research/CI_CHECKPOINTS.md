# Visible checkpoints and production deployments

Each coherent implementation checkpoint should include relevant passing checks,
an evidence/limitations update and a dev-log entry through `tools/update_devlog.py`.
Commit and push at completed component or integration gates. During longer work,
report checkpoint status at least every 30 minutes: last commit, current gate,
remaining checks and blockers. A passing host test is not a deployed checkpoint.

The private repository's [Actions](https://github.com/nrgapple/atv-offroad-fury-2-web/actions)
checks and packages authored source/WASM. On passing main builds, a separate
**Deploy and verify production** job deploys the exact artifact to the
`production` environment. [Deployment history](https://github.com/nrgapple/atv-offroad-fury-2-web/deployments)
records the commit, job outcome and public HTTPS game link. Each run's summary
also links the game, dev log and deployment history. No manual release or
approval is required. GitHub's [environment documentation](https://docs.github.com/en/actions/concepts/workflows-and-actions/deployment-environments)
describes this deployment history integration.

Production success requires the public commit, authored application file hashes,
landing route and committed dev-log text to match the checked CI artifact.
Verification retries briefly for propagation and fails the job on a mismatch.
Deployments are serialized to avoid interrupting a deployment already in flight.
This check has no private-disc dependencies and uploads no original resources.

Screenshot PNGs and required runtime resources remain in the separate R2 package.
The existing host heartbeat validates and updates that package from passing main
checkpoints. Inspect screenshot pixels before publication and verify the public
gallery after its upload. CI verifying dev-log text does not establish screenshot
freshness, gameplay parity or successful browser startup; continue the public
browser and complete R2 verification checkpoints.

On 2026-10-12 02:55 UTC, public and committed dev-log metadata matched main
`1de08b210680c184217bda6b82d5e912a19a6dce`: 50 entries and 45 screenshots.
The implementation checkout had three newer host-only entries for HUD, failed
Nac Nac recovery and prop-ramp wheel support, all explicitly pending publication.
Direct messaging to **Port core gameplay systems** failed with
`already has an active writer`; its latest thread activity showed ongoing tests.
The audit did not stop that writer or commit its dirty game files. These observations
are historical evidence, not a claim that the newer implementation is deployed.
