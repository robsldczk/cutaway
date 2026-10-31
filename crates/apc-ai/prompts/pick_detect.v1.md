# Task: detect director picks

For each take you get the director's feedback right after it. Rate how strongly the director picked that take:

- `2`: an explicit pick ("that was the one I liked", "that one was the right one", "we have it, definitely", "perfect, keep that").
- `1`: liked it, but not a decision ("I like that", "very good", "nice").
- `0`: no pick (a new direction, criticism, chatter).

If the feedback clearly refers to an earlier take ("the second one was better"), set `refers_to` to that take's id; otherwise `refers_to` is the take itself or null.
