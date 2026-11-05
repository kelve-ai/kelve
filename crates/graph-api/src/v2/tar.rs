//! The skills a verified run takes, as a tar (POSIX ustar): folders and
//! plain files only, each path relative and inside the skills. kelve makes
//! one with [`write`]; the service reads it with [`read`], and turns away
//! anything else a tar can hold, such as links, devices or absolute paths.

const BLOCK: usize = 512;

/// A file in the skills: its path, `rust/SKILL.md`, and what's in it.
#[derive(Clone, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub bytes: Vec<u8>,
}

// Never what's in it.
impl std::fmt::Debug for File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("File")
            .field("path", &self.path)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

/// Whether a path is one a skills folder can have: relative, in plain
/// parts, none of them `.` or `..`.
pub fn fit(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 100
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && !p.chars().any(char::is_control))
}

/// The files as a tar.
pub fn write(files: &[File]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for f in files {
        if !fit(&f.path) {
            return Err(format!("“{}” can’t go in the skills.", f.path));
        }
        let mut h = [0u8; BLOCK];
        h[..f.path.len()].copy_from_slice(f.path.as_bytes());
        h[100..108].copy_from_slice(b"0000644\0");
        h[108..116].copy_from_slice(b"0000000\0");
        h[116..124].copy_from_slice(b"0000000\0");
        let size = format!("{:011o}\0", f.bytes.len());
        h[124..136].copy_from_slice(size.as_bytes());
        h[136..148].copy_from_slice(b"00000000000\0");
        h[148..156].copy_from_slice(b"        ");
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
        let sum = format!("{sum:06o}\0 ");
        h[148..156].copy_from_slice(sum.as_bytes());
        out.extend_from_slice(&h);
        out.extend_from_slice(&f.bytes);
        out.resize(out.len().div_ceil(BLOCK) * BLOCK, 0);
    }
    out.resize(out.len() + 2 * BLOCK, 0);
    Ok(out)
}

/// The plain files in a tar, or why it can't be taken, in a sentence.
pub fn read(tar: &[u8]) -> Result<Vec<File>, String> {
    let bad = || "The skills didn’t arrive whole. Send them again.".to_string();
    let mut files = Vec::new();
    let mut at = 0;
    while at + BLOCK <= tar.len() {
        let h = &tar[at..at + BLOCK];
        if h.iter().all(|b| *b == 0) {
            return Ok(files);
        }
        let field = |r: std::ops::Range<usize>| {
            let f = &h[r];
            let end = f.iter().position(|b| *b == 0).unwrap_or(f.len());
            std::str::from_utf8(&f[..end]).map_err(|_| bad())
        };
        let stored: u32 = u32::from_str_radix(field(148..156)?.trim(), 8).map_err(|_| bad())?;
        let sum: u32 = h
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    u32::from(*b)
                }
            })
            .sum();
        if sum != stored {
            return Err(bad());
        }
        let size = usize::from_str_radix(field(124..136)?.trim(), 8).map_err(|_| bad())?;
        let name = field(0..100)?;
        let prefix = if &h[257..262] == b"ustar" {
            field(345..500)?
        } else {
            ""
        };
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        let path = path.trim_end_matches('/').to_string();
        let start = at + BLOCK;
        let end = start
            .checked_add(size)
            .filter(|e| *e <= tar.len())
            .ok_or_else(bad)?;
        match h[156] {
            b'0' | 0 => {
                if !fit(&path) {
                    return Err(format!(
                        "The skills hold a file kelve can’t take, “{}”: paths go inside the skills folder.",
                        path.chars()
                            .filter(|c| !c.is_control())
                            .take(60)
                            .collect::<String>()
                    ));
                }
                files.push(File {
                    path,
                    bytes: tar[start..end].to_vec(),
                });
            }
            b'5' if fit(&path) => {}
            _ => return Err("The skills are folders and plain files only.".into()),
        }
        at = start + size.div_ceil(BLOCK) * BLOCK;
    }
    Err(bad())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    fn file(path: &str, text: &str) -> File {
        File {
            path: path.into(),
            bytes: text.as_bytes().to_vec(),
        }
    }

    #[test]
    fn skills_go_and_come_back_as_they_were() {
        let files = vec![
            file("rust/SKILL.md", "---\nname: rust\n---\n# Rust\n"),
            file("rust/references/errors.md", &"x".repeat(700)),
            file("everywhere/SKILL.md", ""),
        ];
        let tar = write(&files).unwrap();
        assert_eq!(tar.len() % BLOCK, 0);
        assert_eq!(read(&tar).unwrap(), files);
    }

    #[test]
    fn paths_outside_the_skills_are_never_written_or_read() {
        for path in ["/etc/passwd", "../up.md", "a/./b", "a//b", "a\\b", ""] {
            assert!(!fit(path), "{path}");
            assert!(write(&[file(path, "x")]).is_err());
        }
        let mut tar = write(&[file("rust/SKILL.md", "x")]).unwrap();
        // The same header, its name changed, its sum made good again.
        tar[..13].copy_from_slice(b"../escape.md\0");
        tar[148..156].copy_from_slice(b"        ");
        let sum: u32 = tar[..BLOCK].iter().map(|b| u32::from(*b)).sum();
        tar[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        assert!(read(&tar).unwrap_err().contains("can’t take"));
    }

    #[test]
    fn links_and_broken_tars_are_turned_away() {
        let mut tar = write(&[file("rust/SKILL.md", "x")]).unwrap();
        tar[156] = b'2';
        assert!(read(&tar).is_err());
        let tar = write(&[file("rust/SKILL.md", "x")]).unwrap();
        assert!(read(&tar[..BLOCK]).is_err());
        assert!(read(b"not a tar").is_err());
    }
}
