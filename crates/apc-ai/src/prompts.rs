//! Versioned prompt files (`prompts/<task>.v<N>.md`). Bump the version when a prompt
//! changes so cached results are not reused.

use crate::task::AiTask;

pub struct Prompt {
    pub task: AiTask,
    pub version: u32,
    pub text: &'static str,
}

pub fn prompt(task: AiTask) -> Prompt {
    let (version, text) = match task {
        AiTask::LineMatch => (1, include_str!("../prompts/line_match.v1.md")),
        AiTask::RoomClassify => (1, include_str!("../prompts/room_classify.v1.md")),
        AiTask::NoteCondense => (2, include_str!("../prompts/note_condense.v2.md")),
        AiTask::PickDetect => (1, include_str!("../prompts/pick_detect.v1.md")),
        AiTask::SoundSpot => (2, include_str!("../prompts/sound_spot.v2.md")),
        AiTask::SoundTag => (1, include_str!("../prompts/sound_tag.v1.md")),
        AiTask::NameMatch => (2, include_str!("../prompts/name_match.v2.md")),
        AiTask::TakeRescue => (1, include_str!("../prompts/take_rescue.v1.md")),
    };
    Prompt {
        task,
        version,
        text,
    }
}
