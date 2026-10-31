//! Broadcast WAV metadata: `bext` time reference and iXML scene/take/timecode.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::error::Result;

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BwfInfo {
    pub sample_rate: Option<u32>,
    pub time_reference_samples: Option<u64>,
    pub originator: Option<String>,
    pub origination_date: Option<String>,
    pub origination_time: Option<String>,
    pub ixml_scene: Option<String>,
    pub ixml_take: Option<String>,
    pub ixml_tape: Option<String>,
}

impl BwfInfo {
    /// Start timecode in seconds since midnight.
    pub fn tc_start_s(&self) -> Option<f64> {
        match (self.time_reference_samples, self.sample_rate) {
            (Some(t), Some(sr)) if sr > 0 => Some(t as f64 / sr as f64),
            _ => None,
        }
    }
}

fn text(b: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(b);
    let s = s.trim_matches(char::from(0)).trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let a = xml.find(&open)? + open.len();
    let b = xml[a..].find(&close)? + a;
    text(&xml.as_bytes()[a..b])
}

/// Reads BWF chunks. Returns `None` for files that aren't RIFF/WAVE.
pub fn read_bwf(path: &Path) -> Result<Option<BwfInfo>> {
    let mut f = std::fs::File::open(path)?;
    let mut hdr = [0u8; 12];
    if f.read_exact(&mut hdr).is_err() || &hdr[0..4] != b"RIFF" || &hdr[8..12] != b"WAVE" {
        return Ok(None);
    }
    let mut info = BwfInfo::default();
    loop {
        let mut ch = [0u8; 8];
        if f.read_exact(&mut ch).is_err() {
            break;
        }
        let id = [ch[0], ch[1], ch[2], ch[3]];
        let size = u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]) as u64;
        match &id {
            b"fmt " | b"bext" | b"iXML" if size < 1 << 20 => {
                let mut data = vec![0u8; size as usize];
                f.read_exact(&mut data)?;
                match &id {
                    b"fmt " if data.len() >= 8 => {
                        info.sample_rate =
                            Some(u32::from_le_bytes([data[4], data[5], data[6], data[7]]));
                    }
                    b"bext" if data.len() >= 346 => {
                        info.originator = text(&data[256..288]);
                        info.origination_date = text(&data[320..330]);
                        info.origination_time = text(&data[330..338]);
                        let lo =
                            u32::from_le_bytes([data[338], data[339], data[340], data[341]]) as u64;
                        let hi =
                            u32::from_le_bytes([data[342], data[343], data[344], data[345]]) as u64;
                        info.time_reference_samples = Some(lo | (hi << 32));
                    }
                    b"iXML" => {
                        let xml = String::from_utf8_lossy(&data);
                        info.ixml_scene = xml_tag(&xml, "SCENE");
                        info.ixml_take = xml_tag(&xml, "TAKE");
                        info.ixml_tape = xml_tag(&xml, "TAPE");
                    }
                    _ => {}
                }
                if size % 2 == 1 {
                    f.seek(SeekFrom::Current(1))?;
                }
            }
            _ => {
                f.seek(SeekFrom::Current((size + size % 2) as i64))?;
            }
        }
    }
    Ok(Some(info))
}

/// Writes a minimal BWF (for tests and the demo session): PCM16 mono with bext + iXML.
pub fn write_test_bwf(
    path: &Path,
    sr: u32,
    samples: &[f32],
    tc_start_s: f64,
    scene: &str,
) -> Result<()> {
    use std::io::Write;
    let mut bext = vec![0u8; 602];
    bext[256..262].copy_from_slice(b"Cutawy");
    let tr = (tc_start_s * sr as f64) as u64;
    bext[338..342].copy_from_slice(&((tr & 0xffff_ffff) as u32).to_le_bytes());
    bext[342..346].copy_from_slice(&((tr >> 32) as u32).to_le_bytes());
    let ixml =
        format!("<?xml version=\"1.0\"?><BWFXML><SCENE>{scene}</SCENE><TAKE>1</TAKE></BWFXML>");
    let mut ixml_b = ixml.into_bytes();
    if ixml_b.len() % 2 == 1 {
        ixml_b.push(b' ');
    }
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&sr.to_le_bytes());
    fmt.extend_from_slice(&(sr * 2).to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());
    let mut body = Vec::new();
    for (id, chunk) in [
        (b"fmt ", &fmt),
        (b"bext", &bext),
        (b"iXML", &ixml_b),
        (b"data", &data),
    ] {
        body.extend_from_slice(id);
        body.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        body.extend_from_slice(chunk);
        if chunk.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut f = std::fs::File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&((body.len() + 4) as u32).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(&body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_time_reference_and_ixml() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("b.wav");
        write_test_bwf(&p, 48000, &vec![0.1; 4800], 36000.5, "S2").unwrap();
        let i = read_bwf(&p).unwrap().unwrap();
        assert_eq!(i.sample_rate, Some(48000));
        assert_eq!(i.tc_start_s(), Some(36000.5));
        assert_eq!(i.ixml_scene.as_deref(), Some("S2"));
        assert_eq!(i.ixml_take.as_deref(), Some("1"));
        // still decodes as audio
        let a = crate::AudioAnalysis::compute(&p).unwrap();
        assert_eq!(a.env.len(), 10);
    }

    #[test]
    fn non_wav_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.mp3");
        std::fs::write(&p, b"ID3....").unwrap();
        assert!(read_bwf(&p).unwrap().is_none());
    }
}
