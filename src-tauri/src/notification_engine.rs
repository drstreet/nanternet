use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::config::Settings;
use crate::secrets;
use crate::status::{Finding, Level};

const DISCORD_LIMIT: usize = 1900;
const TELEGRAM_LIMIT: usize = 3900;

pub struct Alert {
    pub target: String,
    pub level: Level,
    pub findings: Vec<Finding>,
}

impl Alert {
    pub fn title(&self, language: &str) -> String {
        format!(
            "{} {} — {}",
            badge(self.level),
            self.target,
            severity(self.level, language)
        )
    }

    pub fn body(&self, language: &str) -> String {
        self.findings
            .iter()
            .map(|finding| {
                let headline = headline(&finding.code, language);
                if finding.detail.is_empty() {
                    headline.to_owned()
                } else {
                    format!("{headline}: {}", finding.detail)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub async fn dispatch(app: &AppHandle, settings: &Settings, alert: &Alert) {
    let title = alert.title(&settings.language);
    let body = alert.body(&settings.language);

    if settings.desktop_notifications {
        if let Err(err) = app
            .notification()
            .builder()
            .title(&title)
            .body(&body)
            .show()
        {
            eprintln!("desktop notification failed: {err}");
        }
    }

    let proxy = settings.proxy_url.as_deref();

    if let Some(webhook) = &settings.discord_webhook {
        let payload = json!({ "content": clamp(&format!("**{title}**\n{body}"), DISCORD_LIMIT) });
        post(webhook, &payload, "discord", proxy).await;
    }

    if let Some(chat) = &settings.telegram_chat_id {
        match secrets::read(secrets::TELEGRAM_TOKEN).await {
            Ok(Some(token)) => {
                let payload = json!({
                    "chat_id": chat,
                    "text": clamp(&format!("{title}\n\n{body}"), TELEGRAM_LIMIT),
                    "disable_web_page_preview": true,
                });
                let endpoint = format!("https://api.telegram.org/bot{token}/sendMessage");
                post(&endpoint, &payload, "telegram", proxy).await;
            }
            Ok(None) => eprintln!("telegram chat is configured but no bot token is stored"),
            Err(err) => eprintln!("could not read the telegram token: {err}"),
        }
    }
}

// ponytail: a fresh client per alert rather than one cached per proxy URL.
// Alerts are gated behind the confirmation streak, so this runs on the order of
// once a minute at worst and a connection pool buys nothing. Upgrade path: hold
// a Mutex<Option<(String, Client)>> in AppState keyed on the proxy URL.
async fn post(endpoint: &str, payload: &serde_json::Value, channel: &str, proxy: Option<&str>) {
    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15));

    if let Some(url) = proxy {
        match reqwest::Proxy::all(url) {
            Ok(proxy) => builder = builder.proxy(proxy),
            Err(err) => {
                // Deliberately not falling back to a direct request: the proxy
                // exists because direct does not work, and going around it
                // would send the token over the very path being avoided.
                eprintln!("the {channel} proxy {url} was rejected: {err}");
                return;
            }
        }
    }

    let client = match builder.build() {
        Ok(client) => client,
        Err(err) => {
            eprintln!("could not build the {channel} client: {err}");
            return;
        }
    };

    match client.post(endpoint).json(payload).send().await {
        Ok(response) if !response.status().is_success() => {
            eprintln!(
                "{channel} rejected the alert with HTTP {}",
                response.status()
            );
        }
        Err(err) => eprintln!("{channel} delivery failed: {err}"),
        Ok(_) => {}
    }
}

fn clamp(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    text.chars().take(limit).collect::<String>() + "…"
}

fn badge(level: Level) -> &'static str {
    match level {
        Level::Critical => "🔴",
        Level::Warn => "🟠",
        Level::Ok => "🟢",
        Level::Unknown => "⚪️",
    }
}

fn severity(level: Level, language: &str) -> &'static str {
    let persian = language == "fa";
    match (level, persian) {
        (Level::Critical, true) => "بحرانی",
        (Level::Critical, false) => "critical",
        (Level::Warn, true) => "هشدار",
        (Level::Warn, false) => "warning",
        (Level::Ok, true) => "سالم",
        (Level::Ok, false) => "healthy",
        (Level::Unknown, true) => "نامشخص",
        (Level::Unknown, false) => "unknown",
    }
}

pub fn headline(code: &str, language: &str) -> &'static str {
    if language == "fa" {
        return match code {
            "website.iranAccess" => "سایت ایران‌اکسس شده است",
            "website.blockedLocally" => "از این شبکه مسدود است",
            "website.blockedInsideIran" => "از داخل ایران قابل دسترسی نیست",
            "website.down" => "سایت در دسترس نیست",
            "website.ispPartial" => "بعضی اپراتورهای ایران دسترسی ندارند",
            "website.abroadPartial" => "بعضی نودهای خارج دسترسی ندارند",
            "website.unexpectedStatus" => "کد وضعیت غیرمنتظره",
            "website.contentMismatch" => "محتوای صفحه تغییر کرده",
            "website.externalProbeFailed" => "بررسی خارجی انجام نشد",
            "website.dnsSinkholed" => "دی‌ان‌اس جواب جعلی می‌دهد",
            "website.dnsBypass" => "با تغییر دی‌ان‌اس باز می‌شود",
            "website.dnsUnavailable" => "هیچ دی‌ان‌اسی جواب نداد",
            "website.healthy" => "سایت سالم است",
            "server.unreachable" => "سرور پاسخ نمی‌دهد",
            "server.authenticationFailed" => "ورود SSH رد شد",
            "server.hostKeyMismatch" => "کلید میزبان تغییر کرده است",
            "server.hostKeyLearned" => "کلید میزبان ثبت شد",
            "server.probeFailed" => "اجرای دستور روی سرور شکست خورد",
            "server.loadHigh" => "بار پردازنده بالاست",
            "server.memoryHigh" => "مصرف حافظه بالاست",
            "server.diskHigh" => "فضای دیسک کم است",
            "server.mountMissing" => "مسیر مانت نشده است",
            "server.healthy" => "سرور سالم است",
            "docker.missing" => "کانتینر پیدا نشد",
            "docker.notRunning" => "کانتینر متوقف شده",
            "docker.unhealthy" => "هلث‌چک کانتینر ناسالم است",
            "docker.healthStarting" => "کانتینر در حال آماده شدن است",
            "docker.unavailable" => "داکر پاسخ نمی‌دهد",
            "certificate.expired" => "گواهی SSL منقضی شده",
            "certificate.expiring" => "گواهی SSL نزدیک انقضاست",
            "certificate.valid" => "گواهی SSL معتبر است",
            "certificate.unavailable" => "گواهی SSL خوانده نشد",
            "domain.expired" => "دامنه منقضی شده",
            "domain.expiring" => "دامنه نزدیک انقضاست",
            "domain.valid" => "دامنه معتبر است",
            "domain.lookupFailed" => "استعلام دامنه انجام نشد",
            "domain.registrationUnpublished" => "رجیستری تاریخ انقضا را منتشر نمی‌کند",
            "domain.invalid" => "نام دامنه معتبر نیست",
            "domain.healthy" => "موردی برای بررسی نبود",
            _ => "وضعیت نامشخص",
        };
    }

    match code {
        "website.iranAccess" => "Iran-access isolation detected",
        "website.blockedLocally" => "Blocked on this network",
        "website.blockedInsideIran" => "Unreachable from inside Iran",
        "website.down" => "Site is down",
        "website.ispPartial" => "Some Iranian ISPs cannot reach it",
        "website.abroadPartial" => "Some foreign vantage points failed",
        "website.unexpectedStatus" => "Unexpected status code",
        "website.contentMismatch" => "Page content changed",
        "website.externalProbeFailed" => "External probe unavailable",
        "website.dnsSinkholed" => "DNS is answering with a fake address",
        "website.dnsBypass" => "Another resolver still reaches it",
        "website.dnsUnavailable" => "No resolver answered",
        "website.healthy" => "Site is healthy",
        "server.unreachable" => "Server is not answering",
        "server.authenticationFailed" => "SSH authentication rejected",
        "server.hostKeyMismatch" => "Host key changed",
        "server.hostKeyLearned" => "Host key recorded",
        "server.probeFailed" => "Remote command failed",
        "server.loadHigh" => "CPU load is high",
        "server.memoryHigh" => "Memory usage is high",
        "server.diskHigh" => "Disk space is running out",
        "server.mountMissing" => "Mount point is missing",
        "server.healthy" => "Server is healthy",
        "docker.missing" => "Container not found",
        "docker.notRunning" => "Container is not running",
        "docker.unhealthy" => "Container healthcheck is failing",
        "docker.healthStarting" => "Container is still starting",
        "docker.unavailable" => "Docker is not responding",
        "certificate.expired" => "TLS certificate expired",
        "certificate.expiring" => "TLS certificate expires soon",
        "certificate.valid" => "TLS certificate is valid",
        "certificate.unavailable" => "TLS certificate unreadable",
        "domain.expired" => "Domain registration expired",
        "domain.expiring" => "Domain registration expires soon",
        "domain.valid" => "Domain registration is valid",
        "domain.lookupFailed" => "Registry lookup failed",
        "domain.registrationUnpublished" => "Registry does not publish expiry",
        "domain.invalid" => "Domain name is not valid",
        "domain.healthy" => "Nothing to check",
        _ => "Unknown condition",
    }
}
