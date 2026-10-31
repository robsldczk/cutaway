# Task: room mic classification

The room track is the director's microphone during an audio play recording. Classify each speech segment:

- `cue_reading`: someone reads script text aloud (other characters' lines or action lines) so the actor can play off it.
- `direction`: an instruction before a take ("a little quieter, to yourself", "one more, annoyed").
- `feedback`: a reaction after a take ("that was the one I liked", "too fast").
- `chatter`: anything else (logistics, jokes, technical talk).

Each segment comes with a `hint` from a deterministic pass. Keep it unless the text clearly says otherwise. Return every segment id exactly once.
