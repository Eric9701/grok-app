//! Localized copy emitted directly by the native Remote IM bridge.
//!
//! Catalogs follow `docs/llm-wiki/i18n.md`: every id in `tray_i18n::Locale`,
//! defaulting to English. Language comes from App settings.

use crate::tray_i18n::Locale;

#[derive(Clone, Copy)]
pub enum MessageKey {
    StopSignalSent,
    NoInFlightTurn,
    NoAvailableProject,
    /// `已收到 {name}。`
    AttachmentReceived,
    AttachmentFailed,
    ImageSendFailed,
    /// `/$` wrote the text into the App session and did not start an agent turn.
    NoteRecorded,
    /// `/$` had no text and no waiting attachment.
    NoteNeedsText,
    NoteFailed,
}

/// Canonical catalog id: `en` | `de` | … | `zh-TW`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn normalize_lang(lang: &str) -> &'static str {
    Locale::parse(lang).as_tag()
}

/// Live App locale (`settings.locale`, including `system`).
pub fn resolve_engine_lang() -> String {
    crate::tray_i18n::app_locale().as_tag().to_string()
}

pub fn t(lang: &str, key: MessageKey) -> &'static str {
    let locale = Locale::parse(lang);
    match (locale, key) {
        (Locale::De, MessageKey::StopSignalSent) => "Stoppsignal gesendet.",
        (Locale::De, MessageKey::NoInFlightTurn) => "Kein laufender Zug.",
        (Locale::De, MessageKey::NoAvailableProject) => {
            "Diese Instanz hat kein verfügbares Projekt. Sende /p oder wende dich an einen Admin."
        }

        (Locale::Es, MessageKey::StopSignalSent) => "Señal de parada enviada.",
        (Locale::Es, MessageKey::NoInFlightTurn) => "No hay ningún turno en curso.",
        (Locale::Es, MessageKey::NoAvailableProject) => {
            "Esta instancia no tiene ningún proyecto disponible. Envía /p o contacta a un administrador."
        }

        (Locale::Fil, MessageKey::StopSignalSent) => "Naipadala ang senyas na huminto.",
        (Locale::Fil, MessageKey::NoInFlightTurn) => "Walang kasalukuyang turn.",
        (Locale::Fil, MessageKey::NoAvailableProject) => {
            "Walang available na proyekto ang instance na ito. Magpadala ng /p o makipag-ugnayan sa admin."
        }

        (Locale::Fr, MessageKey::StopSignalSent) => "Signal d’arrêt envoyé.",
        (Locale::Fr, MessageKey::NoInFlightTurn) => "Aucun tour en cours.",
        (Locale::Fr, MessageKey::NoAvailableProject) => {
            "Cette instance n’a aucun projet disponible. Envoyez /p ou contactez un administrateur."
        }

        (Locale::Id, MessageKey::StopSignalSent) => "Sinyal berhenti terkirim.",
        (Locale::Id, MessageKey::NoInFlightTurn) => "Tidak ada giliran yang berjalan.",
        (Locale::Id, MessageKey::NoAvailableProject) => {
            "Instance ini tidak punya proyek yang tersedia. Kirim /p atau hubungi admin."
        }

        (Locale::It, MessageKey::StopSignalSent) => "Segnale di stop inviato.",
        (Locale::It, MessageKey::NoInFlightTurn) => "Nessun turno in corso.",
        (Locale::It, MessageKey::NoAvailableProject) => {
            "Questa istanza non ha progetti disponibili. Invia /p o contatta un amministratore."
        }

        (Locale::Ja, MessageKey::StopSignalSent) => "中断信号を送信しました。",
        (Locale::Ja, MessageKey::NoInFlightTurn) => "進行中のターンはありません。",
        (Locale::Ja, MessageKey::NoAvailableProject) => {
            "このインスタンスに利用可能なプロジェクトがありません。/p を送信するか管理者に連絡してください。"
        }

        (Locale::Ko, MessageKey::StopSignalSent) => "중지 신호를 보냈습니다.",
        (Locale::Ko, MessageKey::NoInFlightTurn) => "진행 중인 턴이 없습니다.",
        (Locale::Ko, MessageKey::NoAvailableProject) => {
            "이 인스턴스에는 사용 가능한 프로젝트가 없습니다. /p 를 보내거나 관리자에게 문의하세요."
        }

        (Locale::PtBr, MessageKey::StopSignalSent) => "Sinal de parada enviado.",
        (Locale::PtBr, MessageKey::NoInFlightTurn) => "Nenhum turno em andamento.",
        (Locale::PtBr, MessageKey::NoAvailableProject) => {
            "Esta instância não tem projeto disponível. Envie /p ou fale com um administrador."
        }

        (Locale::Ru, MessageKey::StopSignalSent) => "Сигнал остановки отправлен.",
        (Locale::Ru, MessageKey::NoInFlightTurn) => "Нет активного хода.",
        (Locale::Ru, MessageKey::NoAvailableProject) => {
            "В этом экземпляре нет доступного проекта. Отправьте /p или обратитесь к администратору."
        }

        (Locale::Ta, MessageKey::StopSignalSent) => "நிறுத்தச் சமிக்ஞை அனுப்பப்பட்டது.",
        (Locale::Ta, MessageKey::NoInFlightTurn) => "நடப்பில் எந்தச் சுற்றும் இல்லை.",
        (Locale::Ta, MessageKey::NoAvailableProject) => {
            "இந்த நிகழ்வில் பயன்படுத்தக்கூடிய திட்டம் இல்லை. /p ஐ அனுப்பவும் அல்லது நிர்வாகியைத் தொடர்பு கொள்ளவும்."
        }

        (Locale::Tr, MessageKey::StopSignalSent) => "Durdurma sinyali gönderildi.",
        (Locale::Tr, MessageKey::NoInFlightTurn) => "Süren bir tur yok.",
        (Locale::Tr, MessageKey::NoAvailableProject) => {
            "Bu örnekte kullanılabilir proje yok. /p gönderin ya da bir yöneticiyle iletişime geçin."
        }

        (Locale::Uk, MessageKey::StopSignalSent) => "Сигнал зупинки надіслано.",
        (Locale::Uk, MessageKey::NoInFlightTurn) => "Немає активного ходу.",
        (Locale::Uk, MessageKey::NoAvailableProject) => {
            "У цьому екземплярі немає доступного проєкту. Надішліть /p або зверніться до адміністратора."
        }

        (Locale::Zh, MessageKey::StopSignalSent) => "已发送中断信号。",
        (Locale::Zh, MessageKey::NoInFlightTurn) => "当前没有进行中的任务。",
        (Locale::Zh, MessageKey::NoAvailableProject) => "当前实例没有可用项目，请发送 /p 或联系管理员。",

        (Locale::ZhTw, MessageKey::StopSignalSent) => "已傳送中斷訊號。",
        (Locale::ZhTw, MessageKey::NoInFlightTurn) => "目前沒有進行中的任務。",
        (Locale::ZhTw, MessageKey::NoAvailableProject) => {
            "目前執行個體沒有可用專案，請傳送 /p 或聯絡管理員。"
        }

        (_, MessageKey::StopSignalSent) => "Stop signal sent.",
        (_, MessageKey::NoInFlightTurn) => "No in-flight turn.",
        (_, MessageKey::NoAvailableProject) => {
            "This instance has no available project. Send /p or contact an admin."
        }
        (_, key) => attachment_copy(locale, key)
            .or_else(|| image_send_copy(locale, key))
            .or_else(|| note_copy(locale, key))
            .unwrap_or(""),
    }
}

fn note_copy(locale: Locale, key: MessageKey) -> Option<&'static str> {
    let recorded = matches!(key, MessageKey::NoteRecorded);
    let needs = matches!(key, MessageKey::NoteNeedsText);
    let failed = matches!(key, MessageKey::NoteFailed);
    if !recorded && !needs && !failed {
        return None;
    }
    Some(match locale {
        Locale::De if recorded => "In dieser Sitzung gespeichert. Der Agent wurde nicht aufgerufen.",
        Locale::De if needs => {
            "Schreib den Text nach `/$`. Er erscheint in der Sitzung und ruft den Agenten nicht auf."
        }
        Locale::De => "Die Nachricht konnte nicht in der Sitzung gespeichert werden.",
        Locale::Es if recorded => "Guardado en esta sesión. No se llamó al agente.",
        Locale::Es if needs => {
            "Escribe el texto después de `/$`. Se muestra en la sesión y no llama al agente."
        }
        Locale::Es => "No se pudo guardar ese mensaje en la sesión.",
        Locale::Fil if recorded => "Na-save sa session na ito. Hindi tinawag ang agent.",
        Locale::Fil if needs => {
            "Isulat ang text pagkatapos ng `/$`. Ipinapakita ito sa session at hindi tumatawag ng agent."
        }
        Locale::Fil => "Hindi ma-save ang mensaheng iyon sa session.",
        Locale::Fr if recorded => "Enregistré dans cette session. L’agent n’a pas été appelé.",
        Locale::Fr if needs => {
            "Écrivez le texte après `/$`. Il s’affiche dans la session et n’appelle pas l’agent."
        }
        Locale::Fr => "Impossible d’enregistrer ce message dans la session.",
        Locale::Id if recorded => "Disimpan di sesi ini. Agen tidak dipanggil.",
        Locale::Id if needs => {
            "Tulis teks setelah `/$`. Pesan tampil di sesi dan tidak memanggil agen."
        }
        Locale::Id => "Pesan itu tidak dapat disimpan di sesi.",
        Locale::It if recorded => "Salvato in questa sessione. L’agente non è stato chiamato.",
        Locale::It if needs => {
            "Scrivi il testo dopo `/$`. Compare nella sessione e non chiama l’agente."
        }
        Locale::It => "Impossibile salvare il messaggio nella sessione.",
        Locale::Ja if recorded => "このセッションに記録しました。Agent は呼び出していません。",
        Locale::Ja if needs => {
            "`/$` の後に表示したい内容を書いてください。セッションにだけ残り、Agent は呼び出しません。"
        }
        Locale::Ja => "このメッセージをセッションに保存できませんでした。",
        Locale::Ko if recorded => "이 세션에 기록했습니다. Agent는 호출하지 않았습니다.",
        Locale::Ko if needs => {
            "`/$` 뒤에 표시할 내용을 쓰세요. 세션에만 보이고 Agent는 호출하지 않습니다."
        }
        Locale::Ko => "이 메시지를 세션에 저장하지 못했습니다.",
        Locale::PtBr if recorded => "Salvo nesta sessão. O agente não foi chamado.",
        Locale::PtBr if needs => {
            "Escreva o texto depois de `/$`. Ele aparece na sessão e não chama o agente."
        }
        Locale::PtBr => "Não foi possível salvar essa mensagem na sessão.",
        Locale::Ru if recorded => "Сохранено в этой сессии. Агент не вызывался.",
        Locale::Ru if needs => {
            "Напишите текст после `/$`. Он появится в сессии и не вызовет агента."
        }
        Locale::Ru => "Не удалось сохранить это сообщение в сессии.",
        Locale::Ta if recorded => "இந்த அமர்வில் சேமிக்கப்பட்டது. Agent அழைக்கப்படவில்லை.",
        Locale::Ta if needs => {
            "`/$` க்குப் பிறகு உரையை எழுதுங்கள். அமர்வில் மட்டும் தோன்றும்; Agent அழைக்கப்படாது."
        }
        Locale::Ta => "இந்த செய்தியை அமர்வில் சேமிக்க முடியவில்லை.",
        Locale::Tr if recorded => "Bu oturuma kaydedildi. Agent çağrılmadı.",
        Locale::Tr if needs => {
            "Metni `/$` sonrasına yazın. Oturumda görünür ve agent çağırmaz."
        }
        Locale::Tr => "Bu mesaj oturuma kaydedilemedi.",
        Locale::Uk if recorded => "Збережено в цій сесії. Агента не викликано.",
        Locale::Uk if needs => {
            "Напишіть текст після `/$`. Він з’явиться в сесії й не викличе агента."
        }
        Locale::Uk => "Не вдалося зберегти це повідомлення в сесії.",
        Locale::Zh if recorded => "已记入当前会话，未调用 Agent。",
        Locale::Zh if needs => "请在 /$ 后面写上要显示的内容。只进入会话，不调用 Agent。",
        Locale::Zh => "这条消息没能写入会话。",
        Locale::ZhTw if recorded => "已記入目前工作階段，未呼叫 Agent。",
        Locale::ZhTw if needs => "請在 /$ 後面寫上要顯示的內容。只進入工作階段，不呼叫 Agent。",
        Locale::ZhTw => "這則訊息沒能寫入工作階段。",
        _ if recorded => "Saved in this session. The agent was not called.",
        _ if needs => {
            "Write the text after `/$`. It is shown in the session and does not call the agent."
        }
        _ => "Couldn't save that message in the session.",
    })
}

fn attachment_copy(locale: Locale, key: MessageKey) -> Option<&'static str> {
    let received = matches!(key, MessageKey::AttachmentReceived);
    let failed = matches!(key, MessageKey::AttachmentFailed);
    if !received && !failed {
        return None;
    }
    Some(match locale {
        Locale::De if received => "{name} empfangen.",
        Locale::De => "Der Anhang konnte nicht heruntergeladen werden.",
        Locale::Es if received => "Se recibió {name}.",
        Locale::Es => "No se pudo descargar el archivo adjunto.",
        Locale::Fil if received => "Natanggap ang {name}.",
        Locale::Fil => "Hindi ma-download ang attachment.",
        Locale::Fr if received => "{name} reçu.",
        Locale::Fr => "Impossible de télécharger la pièce jointe.",
        Locale::Id if received => "{name} diterima.",
        Locale::Id => "Lampiran tidak dapat diunduh.",
        Locale::It if received => "Ricevuto {name}.",
        Locale::It => "Impossibile scaricare l'allegato.",
        Locale::Ja if received => "{name} を受け取りました。",
        Locale::Ja => "添付ファイルをダウンロードできませんでした。",
        Locale::Ko if received => "{name}을(를) 받았습니다.",
        Locale::Ko => "첨부 파일을 받지 못했습니다.",
        Locale::PtBr if received => "{name} recebido.",
        Locale::PtBr => "Não foi possível baixar o anexo.",
        Locale::Ru if received => "Получен файл {name}.",
        Locale::Ru => "Не удалось скачать вложение.",
        Locale::Ta if received => "{name} பெறப்பட்டது.",
        Locale::Ta => "இணைப்பைப் பதிவிறக்க முடியவில்லை.",
        Locale::Tr if received => "{name} alındı.",
        Locale::Tr => "Ek indirilemedi.",
        Locale::Uk if received => "Отримано {name}.",
        Locale::Uk => "Не вдалося завантажити вкладення.",
        Locale::Zh if received => "已收到 {name}。",
        Locale::Zh => "附件下载失败。",
        Locale::ZhTw if received => "已收到 {name}。",
        Locale::ZhTw => "附件下載失敗。",
        _ if received => "Received {name}.",
        _ => "Couldn't download that attachment.",
    })
}

fn image_send_copy(locale: Locale, key: MessageKey) -> Option<&'static str> {
    if !matches!(key, MessageKey::ImageSendFailed) {
        return None;
    }
    Some(match locale {
        Locale::De => "Das Bild konnte nicht gesendet werden.",
        Locale::Es => "No se pudo enviar la imagen.",
        Locale::Fil => "Hindi maipadala ang larawan.",
        Locale::Fr => "Impossible d'envoyer l'image.",
        Locale::Id => "Gambar tidak dapat dikirim.",
        Locale::It => "Impossibile inviare l'immagine.",
        Locale::Ja => "画像を送信できませんでした。",
        Locale::Ko => "이미지를 보내지 못했습니다.",
        Locale::PtBr => "Não foi possível enviar a imagem.",
        Locale::Ru => "Не удалось отправить изображение.",
        Locale::Ta => "படத்தை அனுப்ப முடியவில்லை.",
        Locale::Tr => "Görsel gönderilemedi.",
        Locale::Uk => "Не вдалося надіслати зображення.",
        Locale::Zh => "图片发送失败。",
        Locale::ZhTw => "圖片傳送失敗。",
        _ => "Couldn't send that image.",
    })
}

pub fn format_received(lang: &str, name: &str) -> String {
    t(lang, MessageKey::AttachmentReceived).replace("{name}", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_TAGS: [&str; 16] = [
        "en", "de", "es", "fil", "fr", "id", "it", "ja", "ko", "pt-BR", "ru", "ta", "tr", "uk",
        "zh", "zh-TW",
    ];

    #[test]
    fn stop_messages_cover_all_product_locales() {
        assert_eq!(t("en", MessageKey::StopSignalSent), "Stop signal sent.");
        assert_eq!(
            t("zh", MessageKey::NoInFlightTurn),
            "当前没有进行中的任务。"
        );
        assert_eq!(t("zh-TW", MessageKey::StopSignalSent), "已傳送中斷訊號。");
        assert_eq!(
            t("zh-Hant", MessageKey::NoInFlightTurn),
            "目前沒有進行中的任務。"
        );
        assert_eq!(
            t("zh", MessageKey::NoAvailableProject),
            "当前实例没有可用项目，请发送 /p 或联系管理员。"
        );
        assert!(t("en", MessageKey::NoAvailableProject).contains("/p"));
        assert!(t("zh-TW", MessageKey::NoAvailableProject).contains("/p"));
    }

    #[test]
    fn added_locales_are_translated_not_english() {
        assert_eq!(
            t("ja", MessageKey::StopSignalSent),
            "中断信号を送信しました。"
        );
        assert_eq!(
            t("ko", MessageKey::NoInFlightTurn),
            "진행 중인 턴이 없습니다."
        );
        assert_eq!(t("de", MessageKey::NoInFlightTurn), "Kein laufender Zug.");
        assert_eq!(
            t("pt-BR", MessageKey::StopSignalSent),
            "Sinal de parada enviado."
        );
        assert_eq!(
            t("uk", MessageKey::StopSignalSent),
            "Сигнал зупинки надіслано."
        );
    }

    #[test]
    fn every_locale_answers_and_keeps_the_slash_command() {
        for tag in ALL_TAGS {
            for key in [
                MessageKey::StopSignalSent,
                MessageKey::NoInFlightTurn,
                MessageKey::NoAvailableProject,
                MessageKey::AttachmentReceived,
                MessageKey::AttachmentFailed,
                MessageKey::ImageSendFailed,
                MessageKey::NoteRecorded,
                MessageKey::NoteNeedsText,
                MessageKey::NoteFailed,
            ] {
                assert!(!t(tag, key).trim().is_empty(), "{tag} has empty copy");
            }
            assert!(
                t(tag, MessageKey::NoteNeedsText).contains("/$"),
                "{tag} dropped /$"
            );
            assert!(
                t(tag, MessageKey::AttachmentReceived).contains("{name}"),
                "{tag} dropped the file name"
            );
            // The recovery instruction is useless without the command itself.
            assert!(
                t(tag, MessageKey::NoAvailableProject).contains("/p"),
                "{tag} dropped /p"
            );
        }
    }

    #[test]
    fn unknown_locale_falls_back_to_english() {
        assert_eq!(t("he", MessageKey::StopSignalSent), "Stop signal sent.");
        assert_eq!(
            normalize_lang("system"),
            crate::tray_i18n::Locale::parse("system").as_tag()
        );
        assert_eq!(normalize_lang(""), "en");
        assert_eq!(normalize_lang("zh_CN"), "zh");
        assert_eq!(normalize_lang("zh-TW"), "zh-TW");
        assert_eq!(normalize_lang("ja-JP"), "ja");
        assert_eq!(normalize_lang("pt_PT"), "pt-BR");
    }
}
