//! Decide when a downloaded WeCom file is handed to the agent.
//!
//! A file-only message is held on the chat binding. The next ordinary text
//! turn receives every unconsumed path. Slash commands do not use this path.

use super::control_plane::PendingAttachment;

pub enum AfterDownload {
    /// Every new file failed and there is no caption.
    Failed,
    /// No caption. Replace the binding's pending list with this one.
    Hold(Vec<PendingAttachment>),
    /// Start an agent turn with this caption and these paths, then clear pending.
    Speak {
        text: String,
        attachments: Vec<PendingAttachment>,
    },
}

pub fn after_download(
    text: &str,
    pending: Vec<PendingAttachment>,
    downloaded: Vec<PendingAttachment>,
    had_refs: bool,
) -> AfterDownload {
    let text = text.trim();
    if text.is_empty() {
        if downloaded.is_empty() {
            return if had_refs {
                AfterDownload::Failed
            } else {
                AfterDownload::Hold(pending)
            };
        }
        let mut next = pending;
        next.extend(downloaded);
        return AfterDownload::Hold(next);
    }
    let mut attachments = pending;
    attachments.extend(downloaded);
    AfterDownload::Speak {
        text: text.to_string(),
        attachments,
    }
}

/// Caption plus one sole-line `@absolute` ref per file, matching desktop sends.
pub fn prompt_with_paths(text: &str, attachments: &[PendingAttachment]) -> String {
    let mut out = text.trim().to_string();
    if attachments.is_empty() {
        return out;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    for (i, attachment) in attachments.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push('@');
        out.push_str(attachment.path.trim());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn att(path: &str, name: &str) -> PendingAttachment {
        PendingAttachment {
            path: path.into(),
            name: name.into(),
        }
    }

    #[test]
    fn file_only_is_held_and_does_not_start_a_turn() {
        match after_download("", vec![], vec![att("/w/.remote/1/aa.doc", "aa.doc")], true) {
            AfterDownload::Hold(list) => {
                assert_eq!(list.len(), 1);
                assert_eq!(list[0].name, "aa.doc");
            }
            AfterDownload::Failed | AfterDownload::Speak { .. } => {
                panic!("file-only must wait")
            }
        }
    }

    #[test]
    fn later_text_includes_the_saved_path_and_can_be_cleared() {
        let pending = vec![att("/proj/.remote/1/aa.doc", "aa.doc")];
        match after_download("解析上面的文件", pending, vec![], false) {
            AfterDownload::Speak { text, attachments } => {
                assert_eq!(text, "解析上面的文件");
                let prompt = prompt_with_paths(&text, &attachments);
                assert!(prompt.contains("@/proj/.remote/1/aa.doc"));
                assert!(prompt.starts_with("解析上面的文件\n@"));
            }
            AfterDownload::Failed | AfterDownload::Hold(_) => panic!("text must speak"),
        }
    }

    #[test]
    fn caption_and_file_in_one_message_speak_immediately() {
        match after_download(
            "看这个",
            vec![],
            vec![att("/w/.remote/2/aa.doc", "aa.doc")],
            true,
        ) {
            AfterDownload::Speak { text, attachments } => {
                assert_eq!(text, "看这个");
                assert_eq!(attachments.len(), 1);
            }
            AfterDownload::Failed | AfterDownload::Hold(_) => panic!("caption must speak"),
        }
    }

    #[test]
    fn several_files_keep_arrival_order() {
        let pending = vec![att("/w/.remote/1/aa.doc", "aa.doc")];
        let downloaded = vec![att("/w/.remote/2/bb.png", "bb.png")];
        match after_download("一起看", pending, downloaded, true) {
            AfterDownload::Speak { attachments, .. } => {
                assert_eq!(
                    attachments
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>(),
                    vec!["aa.doc", "bb.png"]
                );
            }
            AfterDownload::Failed | AfterDownload::Hold(_) => panic!("expected speak"),
        }
    }

    #[test]
    fn total_download_failure_does_not_speak() {
        assert!(matches!(
            after_download("", vec![att("/old", "old")], vec![], true),
            AfterDownload::Failed
        ));
    }
}
