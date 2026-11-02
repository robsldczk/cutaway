# Fixtures

Unreleased production material. **Never commit anything in this folder except this README.** It is gitignored.

## tarmac-ep2

13 minutes of the TARMAC Episode 2 recording session, actor PAUL only.

| File | What |
|---|---|
| `tarmac-episode-2_rob-test.fountain` | Script (Throughline export, Fountain) |
| `tarmac-episode-2.thrl` | The final EP02 draft as a Throughline project (gzipped JSON, format 1.12) |
| `tarmac-episode-2_thrl-export.fountain` | Throughline's own Fountain export of that same draft (the `.thrl` parity test compares them) |
| `tarmac-pilot.thrl` | The pilot as a Throughline project (format 1.2): carries the character bios and setting descriptions EP02 lacks |
| `Paul-9min_v001_.mp3` | Paul's mic. Also carries bleed from the director and the Sara reader |
| `Paul-Room-9min_v001_.mp3` | Director room mic. Same timeline, runs about 0.25 s behind Paul's track |
| `Paul-9min_v001_.json` | Segment-level transcript of Paul's mic (diarization labels unreliable) |
| `Paul-Room-9min_v001_.json` | Segment-level transcript of the room mic |

The MP3s are 48 kHz stereo, 782 s. The original WAVs can replace them with the same base names. Expected results are listed in `docs/BUILD_BRIEF.md` (M2) and `docs/reference/stage0/report.md`.
