# Task: match recording files to roles

Audio play sessions have one file per character plus a director room mic. From the file names, the script's characters and known aliases, assign each file a role: `character` (with the character's exact script name), `room` (director, talkback, regie, room mic), or `unknown`. `confidence` is 0..1.

`character_notes`, when present, are the writers' notes per character. They can hold full names, ranks and nicknames ("The boss, Colonel Dean Hunnicutt-Price"): a file named after one of those belongs to that character.
