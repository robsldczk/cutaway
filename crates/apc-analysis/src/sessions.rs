//! Groups files into recording sessions by overlapping timecode (BWF `bext`).
//! Files without timecode that were imported together form one session.

#[derive(Debug, Clone, PartialEq)]
pub struct FileSpan {
    pub id: String,
    pub tc_start_s: Option<f64>,
    pub duration_s: f64,
    /// Import batch (files dropped together).
    pub batch: String,
}

/// A group being built: its timecode span (if any), import batch and file ids.
type Group = (Option<(f64, f64)>, String, Vec<String>);

/// Returns groups of file ids.
pub fn group_sessions(files: &[FileSpan]) -> Vec<Vec<String>> {
    let mut groups: Vec<Group> = Vec::new();
    let mut with_tc: Vec<&FileSpan> = files.iter().filter(|f| f.tc_start_s.is_some()).collect();
    with_tc.sort_by(|a, b| {
        a.tc_start_s
            .unwrap_or(0.0)
            .total_cmp(&b.tc_start_s.unwrap_or(0.0))
    });
    for f in with_tc {
        let s = f.tc_start_s.unwrap_or(0.0);
        let e = s + f.duration_s;
        match groups
            .iter_mut()
            .find(|g| g.0.map(|(gs, ge)| s < ge && gs < e).unwrap_or(false))
        {
            Some(g) => {
                if let Some((gs, ge)) = g.0.as_mut() {
                    *gs = gs.min(s);
                    *ge = ge.max(e);
                }
                g.2.push(f.id.clone());
            }
            None => groups.push((Some((s, e)), f.batch.clone(), vec![f.id.clone()])),
        }
    }
    for f in files.iter().filter(|f| f.tc_start_s.is_none()) {
        match groups.iter_mut().find(|g| g.0.is_none() && g.1 == f.batch) {
            Some(g) => g.2.push(f.id.clone()),
            None => groups.push((None, f.batch.clone(), vec![f.id.clone()])),
        }
    }
    groups.into_iter().map(|g| g.2).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(id: &str, tc: Option<f64>, d: f64, b: &str) -> FileSpan {
        FileSpan {
            id: id.into(),
            tc_start_s: tc,
            duration_s: d,
            batch: b.into(),
        }
    }

    #[test]
    fn overlapping_timecode_groups() {
        let g = group_sessions(&[
            f("paul", Some(36000.0), 600.0, "a"),
            f("room", Some(36000.2), 600.0, "a"),
            f("sara", Some(50000.0), 300.0, "a"),
            f("x", None, 10.0, "b"),
            f("y", None, 10.0, "b"),
        ]);
        assert_eq!(
            g,
            vec![
                vec!["paul".to_string(), "room".into()],
                vec!["sara".into()],
                vec!["x".into(), "y".into()]
            ]
        );
    }
}
