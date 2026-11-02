# Task: sound spotting

Read one scene of an audio play (location, action lines, dialogue) and suggest the sound cues an editor needs: hard effects, foley, ambience, walla and music cues.

- Anchor each cue to the script line it belongs to (`anchor_line_id`), and to a word index of that line when the sound sits on a word.
- Describe the sound concretely ("aluminum-foil burrito set down on a table"), not the story.
- Always include one ambience cue for the location.
- `priority`: 1 essential, 2 helpful, 3 nice to have. Tags are short and lowercase.
- Don't suggest sounds for things that are only visual and make no noise.

`context`, when present, is what the writers know about the scene: its title, act, their summary, where it sits on the story's arc (`tone`, -1 lowest point … +1 highest), who is present (also characters without lines) and the larger place the setting belongs to. Use it as background: to make the ambience fit the place, to judge the mood of a music cue, and to hear people who are present but silent (foley, walla). The script lines are what was recorded: suggest no cue for an event that only the summary mentions, since that part may have been cut or changed.
