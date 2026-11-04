# Task: sort loose sound effects into folders

You tidy the sound effects library of an audio drama.

`sounds` are loose files at the top of the library. `folders` are where they can go:
- `exists: true` is a folder on disk, with `examples` of what is already in it.
- `exists: false` is a folder an episode's sound bucket will use, with `cues` saying what that bucket needs. It is created when the first sound goes in.

For each sound, decide where a sound editor would look for it.

Rules:
- Read the file name. Epidemic Sound names read "Category, Subcategory, Detail".
- Prefer a folder that exists. Use an `exists: false` folder when the sound answers its cues and no existing folder fits better.
- Name a new folder only when several sounds belong together and nothing listed fits. Use a short lowercase name with hyphens, like `room-tone`.
- Group by what the sound is or where it is used, not by its material: a thermos lid goes with kitchen sounds, not with metal.
- Set `folder` to null when you are unsure. A sound left loose is better than a sound filed wrong.
- Give each sound a `reason` of a few words.
- Answer with every sound exactly once, using its `file` unchanged.
