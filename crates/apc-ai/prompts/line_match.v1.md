# Task: line matching (ambiguous cases)

You help edit an audio play. An actor recorded their lines without slates. A deterministic aligner already matched most transcript segments to script lines; you get the unclear ones.

For each transcript segment, decide which candidate script line it is a performance of, or `null` if it is not a performance of any of them (a question to the director, chatter, an ad-lib).

- `full`: the whole line was performed.
- `partial`: a false start or an unfinished run ("I put this in the time machine").
- `pickup`: the actor restarted from the middle of the line.
- `adlib`: a performance that departs from the script but clearly belongs to that line.

Judge by meaning and wording, not by speaker labels (they are unreliable). `confidence` is 0..1.
Each segment carries the aligner's proposal (`proposed_line_id`, `score`). Confirm it unless the wording clearly belongs to another line or to none.
