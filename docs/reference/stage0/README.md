# Stage 0 prototype

`pipeline.py` is the throwaway Python test that proved the matching mechanic on 13 minutes of the TARMAC Ep. 2 session (Paul and the room track). `report.md` is its output.

It ran on sentence-level transcript JSON with estimated word times, and used keyword-based note summaries. Port the logic, not the shortcuts:
- **Matching:** fuzzy matching of transcript windows against *all* dialogue lines. Greedy, non-overlapping, longer lines weighted higher.
- **Short lines:** one- and two-word lines only match whole segments or segment starts.
- **Two passes:** long lines find the active scenes; then all lines are matched within those scenes and their neighbours.
- **Clip edges:** snapped to the role track's loudness envelope (100 frames per second).
- **Overlaps:** prevented between neighbouring takes.
- **Room speech:** speech that matches the script (including action lines) is cue reading; the rest is notes.
- **Note windows:** feedback = 8 s after a take, direction = 30 s before it.
- **Picks:** strong phrases ("that was the one", "right one", "we have the") vs weak phrases ("I like that", "very good").

What the real build changes:
- **Word timing:** word-level ASR (Parakeet) instead of spreading word times evenly across a segment.
- **Notes:** an LLM condenses them, instead of picking keyword sentences.
- **Room sync:** the room track offset is measured and corrected (about 0.25 s in this session).

To run it again: `pip install rapidfuzz numpy`, convert each MP3 to 8 kHz mono float32 raw (`ffmpeg -i X.mp3 -ac 1 -ar 8000 -f f32le src/paul.raw`), and put the files where the `SRC` paths expect them.
