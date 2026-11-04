# Handover: sounds beside the dialogue (Arrange page)

**For:** the agent or developer adding this feature to the Cutaway (Audioplay Cutter) codebase.
**Reference:** the live prototype `BokehLive.dc.html` is attached. It was built in a design canvas to test interaction and feel.

## 1. Ground rules: fit into what exists

This feature joins an app that already exists. Merge it into the app's architecture rather than bolting it on.

- **The codebase wins.** Use the existing document model, command bus, undo log, audio engine, sound bucket, theme tokens and keyboard handling. If the prototype conflicts with something the app already does or the specs define, follow the app. Log the conflict in `DECISIONS.md` (context, choice, reason).
- **Port behavior, not code.** The prototype is a single file built on a canvas runtime, with inline styles, hard-coded pixel positions and fake data. Copy what it *does* and how it *feels*: the interactions, rules, timings and visual hierarchy. Do not copy its structure.
- **One document, many views (D7).** A ball here is the same sound effect clip that the Edit page shows in the sound effects group. Changes in Arrange must show up on Edit, and changes on Edit must show up here. There is no separate data store for this view.
- **Every change is a command.** Moving, trimming, fading, gain, pan, depth and adding or removing a sound all go through the command bus. Each is undoable and reachable from the CLI and MCP (D8).
  - **One undo step per gesture.** A drag produces one command on release, not one per mouse move.
- **Anchoring follows M7.** A sound's start is an anchor to a dialogue word, a line edge or a pause, so the sound follows its line when takes are swapped, gaps change or lines are reordered. The snap targets in the prototype are those anchor targets.
- **Reuse existing components** for the bucket panel, waveforms, tooltips, keyboard hints and the transport. Only the spine, the length lines, the balls and the opened sound view are new.

## 2. What the feature is

Sound effects sit beside the dialogue on the Arrange page, so you can place and shape them while reading the script, without switching to the multitrack Edit page.

From left to right:

1. **Dialogue column.** The existing Arrange line cards, still with their waveform strip (it must not disappear).
2. **Spine.** A thin vertical waveform of the dialogue in each speaker's character color, running between the text and the sounds. It is the timeline: silences show as flat stretches.
3. **Length lines.** One hairline per sound in its bucket color, beside the spine, running from where the sound starts to where it stops. Overlapping sounds sit side by side.
4. **Balls.** Each sound is a ball in the space to the right:
   - size = duration,
   - color = bucket,
   - horizontal position = depth (close to far),
   - vertical position = when it starts.
5. **Bucket panel.** A floating panel on the right, grouped by bucket, used to drag sounds in.

A **Sounds** button (shortcut `S`) hides the whole layer and returns to the plain, centered Arrange view.

## 3. Mapping to the document model

| Prototype | Document (existing or to add) |
|---|---|
| ball | a sound effect clip in the Edit page's sound effects group |
| bucket and its color | the sound bucket category; colors come from theme tokens, not hard-coded |
| start (snap target) | an anchor to a word, line edge or pause (M7); free time when nothing snaps |
| length | clip duration; never longer than the source minus the in point |
| source in point (⌥ drag in the opened sound) | clip source offset ("slip") |
| fade in / fade out | clip fades; fade out is automatic by default (see 5.6) |
| loudness | clip gain in dB, prototype range −24 to +6 dB |
| pan | clip pan, −1 (left) to +1 (right) |
| depth | **new clip parameter, 0 (close) to 1 (far).** See open question 9.1 for the sound behind it |
| bucket hidden (eye) | view state only; never stored in the document |

## 4. Interactions (as in the prototype)

**Placing and moving**
- **Add:** drag a row from the bucket panel beside a line. The drop snaps the start, and the drop position sets the starting depth.
- **Move in time:** drag the ball up or down, or drag the head of its length line. Snaps to line starts, line ends and pauses, with a tooltip naming the word (e.g. *After "toxic."*).
- **Change depth:** drag the ball sideways. Vertical motion is ignored for the first 12 px, so pushing a sound back doesn't nudge its timing.

**Shaping**
- **Trim:** drag the tail of the length line. Snaps to line ends; the fade follows. A dashed line shows the unused rest of the source.
- **Loudness:** turn the ring around the selected ball, or hold ⇧ and scroll over any ball.
- **Pan:** hold ⌥ and drag sideways on a ball.

**Focus and cleanup**
- **Hover** a ball or its line: that sound's length line thickens. Nothing else changes.
- **Select:** click a ball. Its loudness ring appears.
- **Delete:** ⌫ removes the selected sound.
- **Open:** double click a ball or line (or Enter) to open the sound view. Esc or a click outside closes it.
- **Hide a bucket:** click its eye in the panel. ⌥ click shows only that bucket; ⌥ click again restores all.

**Navigation**
- **Play:** Space plays and stops the transport. Sounds play with the dialogue; there are no per-sound play buttons.
- **Scrub:** scroll moves through the scene; ↑ and ↓ jump to the previous or next line.
- **Sounds layer:** `S` or the Sounds button toggles it.

### The opened sound

A lean vertical editor that opens beside the spine. A highlighted wedge connects it to the sound's length line.

- **Time runs vertically**, like the spine, but zoomed to the sound.
- **Left column:** the dialogue waveform in character colors for the same time span, so you can line the sound up with speech.
- **Main column:** the sound's waveform. The unused source parts are dimmed, and the height follows loudness.
- **Edges and corners:**
  - top and bottom edges trim, snapping to line edges and pauses;
  - two corner dots set fade in and fade out;
  - dragging the middle moves the sound; ⌥ drag slips the audio inside the trimmed window.
- **Quick buttons:** End with line, Until next line, Full, Auto fade.
- **Header:** name, bucket, loudness, depth zone and pan.
- **Playhead:** runs through the view during playback.

## 5. Rules worth keeping exactly

1. **Nothing reacts to a drag except the dragged item.** While you drag, the other balls, groups and length lines stay where they are.
   - Settling happens about 0.5 s after release: merges, splits and length-line re-sorting.
   - Clustering and lane assignment therefore read a *settled* snapshot of the sounds, not their live values.
2. **Merging only on real overlap.** Two balls merge into a multicolor group only if they overlap (centers closer than 0.85 × the sum of their radii) *and* their depths differ by less than 0.12.
   - A far ball over a close one never merges.
   - Merging never happens during a drag, and a freshly dropped sound can't merge until the next settle.
3. **Groups split on hover.**
   - Hovering the group spreads its balls in time order; they stay apart while the pointer is near and close about 0.4 s after it leaves.
   - A group with the selected, open or dragged sound stays split.
4. **Sticky lanes.** Length lines keep their position beside the spine; only a new or moved sound looks for a free one. Adding a sound must never shuffle the others. Very long sounds (room tone, walla) get their own outer lane.
5. **No dimming.** Other balls keep full brightness on hover, select and drag. Only a hidden bucket fades.
6. **Auto fade out by default:** `min(0.3 s, 30 % of the length)`, 2 s for beds over 8 s. Setting a fade by hand turns auto off for that sound.
7. **Hidden buckets** fade to about 12 % opacity, blurred, with no labels or connecting lines. They can't be hovered or grabbed and never merge. A selected or open sound in a hidden bucket is deselected or closed.

## 6. Motion

Anything you see dozens of times per session is calm. Only rare moments may be playful.

| Moment | Spec |
|---|---|
| length line thickens on hover | 0.3 s, `cubic-bezier(.25,.8,.25,1)`, fade in and widen from 55 %, no overshoot; knob and tail handle fade in over 0.3 s |
| loudness ring on select | draws once around the ball, 0.7 s ease-out with a fade; knob fades in after 0.45 s |
| merge, split, settle | 0.6 s ease-out on position, 0.4 s on opacity, small stagger (30 ms) when splitting; no spring |
| dragged ball | follows the pointer with about 0.12 s of smoothing |
| Sounds toggle *(playful, rare)* | balls shrink into the spine one by one while the dialogue glides to the center and the panel slides out; on return they pop out in time order with a slight overshoot |
| new sound dropped *(playful, rare)* | pops in with a slight overshoot |
| merged group | slow conic swirl of its members' colors, gentle wobble, count badge |

Respect the system's reduced-motion setting: drop the swirl, wobble and overshoots.

## 7. Why it is designed this way

- **Vertical, beside the script.** Arrange is a reading page. Placing sounds next to the words they belong to keeps the writer's head in the story. The Edit page remains the precise multitrack tool.
- **The spine is a dialogue waveform.** The vertical layout isn't proportional to time, so the spine shows timing honestly: who speaks, where lines end, where the silences are. Silences are where sounds usually go, which is also why snapping targets them.
- **Character colors on the spine, bucket colors on sounds.** Two separate color systems keep "who is talking" and "what kind of sound" readable at a glance without a legend.
- **Balls instead of clip blocks.** Sounds are placed by feel here, not frame by frame. Size, position and blur carry duration, depth and presence without numbers. The fake 3D is the mental model for a mix: close is crisp and saturated, far is small, soft and desaturated.
- **Length as a hairline, thick only in focus.** A scene can hold dozens of sounds. Hairlines show coverage and overlaps without clutter. Focusing one sound reveals its exact span, fade and handles.
- **Ring = loudness, sideways = depth, ⌥ = pan.**
  - **Depth** is the most frequent, spatial gesture, so it gets the plain drag.
  - **Loudness** behaves like a rotary knob, which audio people already know, and ⇧ scroll is the fast path.
  - **Pan** is less frequent and would collide with the depth drag, so it sits behind ⌥, in line with Logic's habit of modifiers for alternate behavior.
  - **The pan line** on each ball shows the current pan like a globe turning; it is never blurred, so it stays readable on far balls.
- **No play buttons.** Sounds play when the dialogue reaches them, so you judge them in context. Balls pulse slightly while their sound plays.
- **Calm after release.** Earlier versions merged and reshuffled while dragging, which made it impossible to predict what would happen and made the screen busy. Deferring all layout changes to a short settle after release keeps the gesture predictable.
- **Strict merging.** Merging exists only to stop overlapping balls from hiding each other. Merging by proximity alone, or across depths, surprised users, so it only happens when balls physically share space.
- **Opened sound is vertical.** It keeps the same time direction as the spine, so trimming against a word feels the same in both. Showing the dialogue beside it removes guesswork when aligning to speech.
- **Bucket eyes.** Sound work happens in passes (food and foil first, then room). Fading other buckets rather than hiding them keeps orientation while removing clutter. ⌥ click to solo is the familiar exclusive-solo shortcut.
- **Sounds toggle.** Some sessions are only about dialogue. One key returns to the pure Arrange view without losing any sound placement.

## 8. Implementation notes

- **Spine scale.** The vertical position comes from the Arrange layout (cards and gap rows), not from linear time. Map time to y within each card and gap, and treat the spacing between cards as zero time. The same mapping drives the spine, the length lines and the snapping.
- **Snap targets:** line start, line end and internal pauses (from word timings). The tooltip names the nearest word. Snap distance is about 8 px on the spine, or an equivalent time in the opened view.
- **Performance.** Render the spine and length lines as a few paths (one per character and one per bucket), not one element per sound. Freeze clustering and lanes during drags (rule 5.1), which also avoids layout work per frame.
- **Accessibility.**
  - Every ball is keyboard reachable and announces its name, bucket, start, length, loudness, pan and depth.
  - Ring, pan and depth need keyboard equivalents, e.g. ←/→ for depth, ⌥←/→ for pan, ⇧↑/↓ for loudness. Pick keys that fit the app's existing map and record them.
- **CLI and MCP.** Add `gain`, `pan`, `depth`, fades and the anchor to the sound commands so an agent can do everything the mouse can.

## 9. Open questions

1. **What depth sounds like.** The prototype is visual only. Proposal: depth drives level offset, a gentle high-frequency roll-off and the reverb send, as one macro. Rob decides the curve.
2. **More than five overlapping short sounds** fall into a shared lane in the prototype. Decide whether to add lanes or show a "+n" marker.
3. **Multi-select** and moving several sounds at once are not designed yet.
4. **Merged group order.** Groups split in time order. Confirm that this is right when members share a start.
5. **Where bucket visibility lives:** per page, per scene or global. The prototype treats it as session view state.

## 10. Done when

- Sounds placed, moved, trimmed, faded, panned and pushed back on Arrange appear identically on the Edit page, and the reverse.
- Each gesture is one undo step, and the same actions work through the CLI and MCP.
- Swapping a take or reordering lines moves the anchored sounds with their lines.
- While dragging, no other ball, group or length line moves; merges happen only after release and only under rule 5.2.
- Adding a sound from the bucket does not change the position of any other length line.
- Hiding a bucket fades only that bucket; ⌥ click solos it.
- Animations match section 6 and respect reduced motion.
- Every conflict with the existing app is resolved in favor of the app and logged in `DECISIONS.md`.
