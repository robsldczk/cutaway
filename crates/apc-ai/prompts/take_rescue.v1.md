# Task: rescue misheard takes

You help edit an audio play. Each actor recorded their lines without slates, many times over, on their own session mic. A director mic in the room caught the director's notes and a reader giving cues. Speech recognition turned both mics into text, and a matcher linked the clear reads to the script. Speech recognition mishears words, so some real takes were left over.

You get one scene: its script (every character, so you can follow the dialogue) and the recording of one actor's session, as it happened, sentence by sentence:

- `s…` sentences are from the actor's session mic. Only these can be takes.
- `d…` sentences are from the director mic: direction, feedback, cue reading, chatter. They are context only and never a take, even when they read like a script line.
- `[= L3]` marks a sentence the matcher already took as a take of L3.
- `[?]` marks a session-mic sentence the matcher left over. Decide for each whether it is a take.
- `[bleed]` is not the actor's own voice. Leave it.
- `— 12 s —` is a pause.

For every `[?]` sentence (and every id the editor marked), answer:

- `line`: the label of the script line this sentence performs, or `null` if it is not a take (a question, a comment, chatter, a cough, the actor reading someone else's cue for themselves).
- Only the actor's own lines (`CHARACTER`) can be takes.
- A take may be misheard badly: judge by sound-alike words, rhythm, length and where it sits (between other takes of the same line, right after the director asked for another one). A wording the actor changed slightly is still a take of that line.
- Short lines ("Paul!", "What?") are only takes when they sit where a read of that line belongs, not when they are chatter.
- `read_as`: what the actor most likely said, in the script's spelling.
- `confidence`: `high` when the context leaves no doubt, `medium` when it is likely, `low` when it is a guess.
- When only part of a sentence is the take (it runs on into "sorry, again"), give `from` and `to`: the first and last words of the take, quoted exactly as they appear in that sentence.

Answer only for `[?]` sentences and marked ids. Use only ids from the recording.
