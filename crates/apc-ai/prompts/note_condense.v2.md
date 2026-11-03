# Task: what the room said between takes

The room track is the director's microphone during an audio play recording. Between two takes the people in the room talk: the director reacts to the take that just ended, sets up the next one, and logistics, jokes and technical talk run through it all. The transcript is unpunctuated speech-to-text and can be garbled; reading cues of the script are already removed.

Each gap comes with everything said between the previous take and the next one, in order, plus those takes' character and script line. `previous` is missing before a session's first take, `next` after its last.

Write `summary`: **one or two full sentences** an editor can read at a glance, saying what the people in the room meant. Read the whole gap as one conversation and interpret it:

- how they felt about the previous take, and why,
- what they asked for in the next one, and how,
- anything else that matters to the edit: a technical problem, a decision, a reason to go again, a line change.

Good: "The director liked the energy but felt the end was rushed, and asked for the next one quieter, as if Paul were talking to himself."
Good: "A cable crackled during the take, so they went again straight away without notes."
Bad (keywords): "quieter, to yourself, more energy".
Bad (a quote): "yeah um that was good let's do one more maybe a bit".

Leave out small talk that explains nothing. Name people only when the text makes it clear who spoke ("the director", or the actor's character). Use an empty string when nothing relevant was said. Never invent what isn't in the text.

Add short lowercase tags (e.g. `quieter`, `louder`, `energy`, `emphasis`, `annoyed`, `excited`, `re-take`, `technical`, `flub`, `ad-lib`, `pick`, `liked`). Return every gap id exactly once.
