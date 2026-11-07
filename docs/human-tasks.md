# Tasks for Rob

Only tasks that need accounts, credentials, hardware, a listening test or another app. Before you add a task, check three things: is it a text file, is there a CLI flag, can a shell verify it. If yes, do it yourself.

Delete a task when it is done. The full steps for the first two are in `docs/RUN_REPORT.md`, *What Rob needs to do*.

- [ ] Import the FCPXML in Resolve and check the timeline. Repeat in Logic Pro if you use it there.
- [ ] Add the MCP server to Claude Code and check that the open app follows it.
- [ ] Run the live Claude Code test: `cargo test -p apc-ai -- --ignored live_pick_detection`.
- [ ] In the app built by the Dock app (WebKit), check by hand what six UI tests found (`docs/STATUS.md`, *Known problems*): Escape after a right click on an Arrange line, Escape in the pan popover, dragging a pair's offset pill, dragging a review note by its card, and the tour's drag on the grip. Remove the `test.fail` line of each one that works.

