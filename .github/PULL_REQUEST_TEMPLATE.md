## What and why

<!-- What does this change, and why is it needed? Link the issue: "Fixes #123". -->

## How it was tested

<!-- Automated checks, plus hardware testing where relevant. -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --locked --all-targets -- -D warnings`
- [ ] `cargo test --locked`
- [ ] Tested on hardware. Mac and macOS version: … Phone and Android version: …
- [ ] No leftovers after stopping (`ifconfig -l`, `networksetup -listnetworkserviceorder`)
- [ ] Performance-sensitive change: `scripts/bench.sh` results before and after are included below

## Checklist

- [ ] [ARCHITECTURE.md](https://github.com/frosty-universe/usb-android-tethering/blob/main/docs/ARCHITECTURE.md) invariants still hold (teardown order, transfer ownership, service lifecycle)
- [ ] User-visible change recorded in `CHANGELOG.md` under **Unreleased**
- [ ] Docs updated (README / docs/) if behaviour or commands changed
- [ ] No new dependencies, or the reason is explained above
