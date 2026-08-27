use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::config::{ServerSpec, Settings, Target, TargetSpec};
use crate::secrets;
use crate::status::{
    DnsAttempt, DnsReport, DomainReport, Finding, Level, LocalProbe, NodeResult, ServerReport,
    TargetStatus,
};

const DISCORD_LIMIT: usize = 1900;
const TELEGRAM_LIMIT: usize = 3900;
const DETAIL_LIMIT: usize = 160;
const MAX_LINES: usize = 8;
const MAX_ROWS: usize = 16;
const CELL_LIMIT: usize = 44;
const LATENCY_BEST: u64 = 500;
const LATENCY_GOOD: u64 = 1000;
const LATENCY_FAIR: u64 = 2000;

pub struct Alert {
    pub target: String,
    pub subject: Option<String>,
    pub level: Level,
    pub findings: Vec<Finding>,
    pub table: Vec<Vec<String>>,
    pub report: Option<String>,
}

impl Alert {
    pub fn of(target: &Target, status: &TargetStatus) -> Self {
        Self {
            target: target.name.clone(),
            subject: Some(subject(&target.spec)),
            level: status.level,
            findings: status.findings.clone(),
            table: rows(&target.spec, status),
            report: status
                .website
                .as_ref()
                .and_then(|site| site.external.as_ref())
                .and_then(|probe| probe.permanent_link.clone()),
        }
    }

    pub fn note(target: &str, level: Level, findings: Vec<Finding>) -> Self {
        Self {
            target: target.to_owned(),
            subject: None,
            level,
            findings,
            table: Vec::new(),
            report: None,
        }
    }

    pub fn title(&self, language: &str) -> String {
        format!(
            "{} {} — {}",
            badge(self.level),
            self.target,
            severity(self.level, language)
        )
    }

    fn lines(&self, language: &str) -> Vec<(String, Option<String>)> {
        let mut lines: Vec<_> = self
            .findings
            .iter()
            .take(MAX_LINES)
            .map(|finding| {
                (
                    format!(
                        "{} {}",
                        badge(finding.level),
                        headline(&finding.code, language)
                    ),
                    (!finding.detail.is_empty()).then(|| clamp(&finding.detail, DETAIL_LIMIT)),
                )
            })
            .collect();
        if self.findings.len() > MAX_LINES {
            lines.push((format!("… +{}", self.findings.len() - MAX_LINES), None));
        }
        lines
    }

    pub fn prose(&self, language: &str) -> String {
        self.lines(language)
            .into_iter()
            .map(|(headline, detail)| match detail {
                Some(detail) => format!("{headline}\n{}", isolate(&detail)),
                None => headline,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub fn text(&self, language: &str, fence: bool) -> String {
        let mut out = self.title(language);
        if let Some(subject) = &self.subject {
            out.push_str(&format!("\n{}", isolate(subject)));
        }
        out.push_str(&format!("\n\n{}", self.prose(language)));
        if let Some(block) = self.block() {
            out.push_str(&if fence {
                format!("\n\n```\n{block}\n```")
            } else {
                format!("\n\n{block}")
            });
        }
        if let Some(report) = &self.report {
            out.push_str(&format!("\n\n{report}"));
        }
        out
    }

    pub fn rich_html(&self, language: &str) -> String {
        let mut out = format!("<h3>{}</h3>", escape(&self.title(language)));
        if let Some(subject) = &self.subject {
            out.push_str(&format!("<p><code>{}</code></p>", cell(subject)));
        }
        for (headline, detail) in self.lines(language) {
            out.push_str(&format!("<p><b>{}</b>", escape(&headline)));
            if let Some(detail) = detail {
                out.push_str(&format!("<br><code>{}</code>", cell(&detail)));
            }
            out.push_str("</p>");
        }

        if let Some(rows) = self.grid() {
            out.push_str("<table bordered striped compact>");
            for (index, row) in rows.iter().enumerate() {
                let tag = if index == 0 { "th" } else { "td" };
                out.push_str("<tr>");
                for value in row {
                    out.push_str(&format!("<{tag}>{}</{tag}>", cell(value)));
                }
                out.push_str("</tr>");
            }
            out.push_str("</table>");
        }
        if let Some(report) = &self.report {
            out.push_str(&match https(report) {
                Some(url) => format!(
                    "<p><a href=\"{}\">{}</a></p>",
                    escape(url),
                    escape(report_label(language))
                ),
                None => format!("<p>{}</p>", escape(report)),
            });
        }
        out
    }

    pub fn html(&self, language: &str) -> String {
        let mut out = format!("<b>{}</b>", escape(&self.title(language)));
        if let Some(subject) = &self.subject {
            out.push_str(&format!("\n<code>{}</code>", cell(subject)));
        }
        for (headline, detail) in self.lines(language) {
            out.push_str(&format!("\n\n<b>{}</b>", escape(&headline)));
            if let Some(detail) = detail {
                out.push_str(&format!("\n<i>{}</i>", cell(&detail)));
            }
        }

        if let Some(block) = self.block() {
            out.push_str(&format!("\n\n<pre>{}</pre>", cell(&block)));
        }
        if let Some(report) = &self.report {
            out.push_str(&match https(report) {
                Some(url) => format!(
                    "\n\n<a href=\"{}\">{}</a>",
                    escape(url),
                    escape(report_label(language))
                ),
                None => format!("\n\n{}", escape(report)),
            });
        }
        out
    }

    fn grid(&self) -> Option<Vec<Vec<String>>> {
        (self.table.len() > 2).then(|| {
            self.table
                .iter()
                .take(MAX_ROWS + 1)
                .map(|row| row.iter().map(|value| clamp(value, CELL_LIMIT)).collect())
                .collect()
        })
    }

    fn block(&self) -> Option<String> {
        self.grid().map(|rows| table(&rows))
    }

    pub fn telegram_payload(&self, language: &str) -> (String, bool) {
        let html = self.html(language);
        if html.chars().count() <= TELEGRAM_LIMIT {
            (html, true)
        } else {
            (clamp(&self.text(language, false), TELEGRAM_LIMIT), false)
        }
    }
}

pub async fn dispatch(app: &AppHandle, settings: &Settings, alert: &Alert) {
    if settings.desktop_notifications {
        if let Err(err) = app
            .notification()
            .builder()
            .title(alert.title(&settings.language))
            .body(alert.prose(&settings.language))
            .show()
        {
            eprintln!("desktop notification failed: {err}");
        }
    }

    let proxy = settings.proxy_url.as_deref();

    if let Some(webhook) = &settings.discord_webhook {
        let payload =
            json!({ "content": clamp(&alert.text(&settings.language, true), DISCORD_LIMIT) });
        post(webhook, &payload, "discord", proxy).await;
    }

    if settings.telegram_chat_ids.is_empty() {
        return;
    }

    let token = match secrets::read(secrets::TELEGRAM_TOKEN).await {
        Ok(Some(token)) => token,
        Ok(None) => {
            eprintln!("telegram chats are configured but no bot token is stored");
            return;
        }
        Err(err) => {
            eprintln!("could not read the telegram token: {err}");
            return;
        }
    };

    let message = Telegram {
        rich: alert.rich_html(&settings.language),
        plain: alert.telegram_payload(&settings.language),
        rtl: settings.language == "fa",
        proxy: settings.proxy_url.clone(),
    };

    let mut sending = tokio::task::JoinSet::new();
    for chat in settings.telegram_chat_ids.clone() {
        let token = token.clone();
        let message = message.clone();
        sending.spawn(async move { message.send(&token, &chat).await });
    }
    while sending.join_next().await.is_some() {}
}

#[derive(Clone)]
struct Telegram {
    rich: String,
    plain: (String, bool),
    rtl: bool,
    proxy: Option<String>,
}

impl Telegram {
    async fn send(&self, token: &str, chat: &str) {
        let api = format!("https://api.telegram.org/bot{token}");
        let proxy = self.proxy.as_deref();
        let rich = json!({
            "chat_id": chat,
            "rich_message": {
                "html": self.rich,
                "is_rtl": self.rtl,
                "skip_entity_detection": true,
            },
        });

        if post(&format!("{api}/sendRichMessage"), &rich, "telegram", proxy).await {
            return;
        }

        let (text, markup) = &self.plain;
        let mut payload = json!({
            "chat_id": chat,
            "text": text,
            "disable_web_page_preview": true,
        });
        if *markup {
            payload["parse_mode"] = json!("HTML");
        }
        post(&format!("{api}/sendMessage"), &payload, "telegram", proxy).await;
    }
}

fn subject(spec: &TargetSpec) -> String {
    match spec {
        TargetSpec::Website(spec) => spec.url.clone(),
        TargetSpec::Server(spec) => format!("{}:{}", spec.host, spec.port),
        TargetSpec::Domain(spec) => spec.domain.clone(),
    }
}

fn rows(spec: &TargetSpec, status: &TargetStatus) -> Vec<Vec<String>> {
    match spec {
        TargetSpec::Website(_) => website_rows(status),
        TargetSpec::Server(spec) => status
            .server
            .as_ref()
            .map(|report| server_rows(spec, report))
            .unwrap_or_default(),
        TargetSpec::Domain(spec) => status
            .domain
            .as_ref()
            .map(|report| domain_rows(report, spec.warn_days))
            .unwrap_or_default(),
    }
}

fn website_rows(status: &TargetStatus) -> Vec<Vec<String>> {
    let Some(report) = &status.website else {
        return Vec::new();
    };
    let mut rows = vec![cells(["ORIGIN", "VANTAGE POINT", "PING", "RESULT"])];

    let local = &report.local;
    rows.push(vec![
        graded(grade_local(local), "LOCAL"),
        "your network".to_owned(),
        millis(local.latency_ms),
        match (local.status, &local.error) {
            (Some(code), _) => code.to_string(),
            (None, Some(error)) => error.clone(),
            (None, None) => "no response".to_owned(),
        },
    ]);

    for node in report
        .external
        .iter()
        .flat_map(|probe| probe.nodes.iter())
        .take(MAX_ROWS)
    {
        rows.push(vec![
            graded(grade_node(node), node.region()),
            node.label(),
            millis(node.latency_ms),
            match (&node.status, &node.message, node.reachable) {
                (Some(code), _, _) => code.clone(),
                (None, Some(message), _) => message.clone(),
                (None, None, None) => "still running".to_owned(),
                (None, None, _) => "no answer".to_owned(),
            },
        ]);
    }

    rows.extend(dns_rows(status.dns.as_ref()));
    rows
}

fn dns_rows(report: Option<&DnsReport>) -> Vec<Vec<String>> {
    report
        .into_iter()
        .flat_map(|report| report.attempts.iter())
        .take(MAX_ROWS)
        .map(|attempt| {
            vec![
                graded(grade_resolver(attempt), "DNS"),
                format!("{} {}", attempt.label, attempt.resolver),
                millis(attempt.probe.as_ref().and_then(|probe| probe.latency_ms)),
                match (&attempt.error, attempt.reserved, &attempt.probe) {
                    (Some(error), _, _) => error.clone(),
                    (None, true, _) => "filtered answer".to_owned(),
                    (None, false, Some(probe)) => match (probe.status, &probe.error) {
                        (Some(code), _) => code.to_string(),
                        (None, Some(error)) => error.clone(),
                        (None, None) => "no response".to_owned(),
                    },
                    (None, false, None) => "no address".to_owned(),
                },
            ]
        })
        .collect()
}

fn server_rows(spec: &ServerSpec, report: &ServerReport) -> Vec<Vec<String>> {
    let mut rows = vec![cells(["METRIC", "VALUE", "LIMIT"])];
    if let Some(per_core) = report.load_per_core {
        rows.push(vec![
            graded(grade_ratio(per_core, spec.load_limit), "load per core"),
            format!("{per_core:.2}"),
            format!("{:.2}", spec.load_limit),
        ]);
    }
    if let Some(memory) = report.memory {
        rows.push(vec![
            graded(grade_ratio(memory.percent, spec.memory_limit), "memory"),
            format!("{:.0}%", memory.percent),
            format!("{:.0}%", spec.memory_limit),
        ]);
    }
    for disk in report.disks.iter().take(MAX_ROWS) {
        rows.push(vec![
            graded(
                grade_ratio(disk.usage.percent, spec.disk_limit),
                &format!("disk {}", disk.mount),
            ),
            format!("{:.0}%", disk.usage.percent),
            format!("{:.0}%", spec.disk_limit),
        ]);
    }
    for container in report.containers.iter().take(MAX_ROWS) {
        rows.push(vec![
            graded(
                grade_container(&container.state, container.health.as_deref()),
                &format!("box {}", container.name),
            ),
            match &container.health {
                Some(health) => format!("{} ({health})", container.state),
                None => container.state.clone(),
            },
            String::new(),
        ]);
    }
    rows
}

fn domain_rows(report: &DomainReport, warn: i64) -> Vec<Vec<String>> {
    let mut rows = vec![cells(["CHECK", "SOURCE", "DAYS LEFT"])];
    if let Some(certificate) = &report.certificate {
        rows.push(vec![
            graded(grade_days(certificate.days_left, warn), "certificate"),
            certificate.issuer.clone(),
            certificate.days_left.to_string(),
        ]);
    }
    if let Some(registration) = &report.registration {
        rows.push(vec![
            graded(grade_days(registration.days_left, warn), "registration"),
            registration.source.clone(),
            registration.days_left.to_string(),
        ]);
    }
    rows
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grade {
    Best,
    Good,
    Fair,
    Poor,
    Bad,
    Pending,
}

impl Grade {
    fn dot(self) -> &'static str {
        match self {
            Grade::Best => "🟢",
            Grade::Good => "🔵",
            Grade::Fair => "🟡",
            Grade::Poor => "🟠",
            Grade::Bad => "🔴",
            Grade::Pending => "⚪",
        }
    }
}

fn graded(grade: Grade, label: &str) -> String {
    format!("{} {label}", grade.dot())
}

fn grade_latency(latency: Option<u64>) -> Grade {
    match latency {
        Some(ms) if ms < LATENCY_BEST => Grade::Best,
        Some(ms) if ms < LATENCY_GOOD => Grade::Good,
        Some(ms) if ms < LATENCY_FAIR => Grade::Fair,
        Some(_) => Grade::Poor,
        None => Grade::Pending,
    }
}

fn grade_local(probe: &LocalProbe) -> Grade {
    if !probe.reachable {
        return Grade::Bad;
    }
    if probe.status.is_some_and(|code| code >= 400) {
        return Grade::Poor;
    }
    grade_latency(probe.latency_ms)
}

fn grade_node(node: &NodeResult) -> Grade {
    match node.reachable {
        None => Grade::Pending,
        Some(false) => Grade::Bad,
        Some(true) => {
            let failed = node
                .status
                .as_deref()
                .and_then(|code| code.parse::<u16>().ok())
                .is_some_and(|code| code >= 400);
            if failed {
                Grade::Poor
            } else {
                grade_latency(node.latency_ms)
            }
        }
    }
}

fn grade_resolver(attempt: &DnsAttempt) -> Grade {
    if attempt.error.is_some() || attempt.reserved {
        return Grade::Bad;
    }
    match &attempt.probe {
        Some(probe) => grade_local(probe),
        None => Grade::Pending,
    }
}

fn grade_ratio(value: f64, limit: f64) -> Grade {
    if !limit.is_finite() || limit <= 0.0 {
        return Grade::Pending;
    }
    match value / limit {
        ratio if ratio >= 1.0 => Grade::Bad,
        ratio if ratio >= 0.9 => Grade::Poor,
        ratio if ratio >= 0.75 => Grade::Fair,
        ratio if ratio >= 0.5 => Grade::Good,
        _ => Grade::Best,
    }
}

fn grade_days(days: i64, warn: i64) -> Grade {
    let warn = warn.max(1);
    match days {
        left if left <= 0 => Grade::Bad,
        left if left <= warn => Grade::Poor,
        left if left <= warn * 2 => Grade::Fair,
        left if left <= warn * 4 => Grade::Good,
        _ => Grade::Best,
    }
}

fn grade_container(state: &str, health: Option<&str>) -> Grade {
    if !state.eq_ignore_ascii_case("running") {
        return Grade::Bad;
    }
    match health {
        Some(health) if health.eq_ignore_ascii_case("healthy") => Grade::Best,
        Some(health) if health.eq_ignore_ascii_case("starting") => Grade::Fair,
        Some(_) => Grade::Poor,
        None => Grade::Best,
    }
}

fn cells<const N: usize>(values: [&str; N]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn millis(latency: Option<u64>) -> String {
    match latency {
        Some(ms) => format!("{ms} ms"),
        None => "-".to_owned(),
    }
}

fn table(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|value| width(value))
                .max()
                .unwrap_or(0)
        })
        .collect();

    rows.iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(column, value)| {
                    if column + 1 == row.len() {
                        value.clone()
                    } else {
                        let pad = widths[column].saturating_sub(width(value));
                        format!("{value}{}", " ".repeat(pad))
                    }
                })
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn width(text: &str) -> usize {
    text.chars()
        .map(|character| match character {
            '\u{2600}'..='\u{27BF}' | '\u{1F300}'..='\u{1FAFF}' => 2,
            _ => 1,
        })
        .sum()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn cell(text: &str) -> String {
    escape(&isolate(text))
}

fn isolate(text: &str) -> String {
    if text.is_empty() || rightward(text) {
        return text.to_owned();
    }
    format!("\u{2066}{text}\u{2069}")
}

fn rightward(text: &str) -> bool {
    text.chars().any(|character| {
        matches!(character,
            '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
    })
}

fn https(url: &str) -> Option<&str> {
    url.starts_with("https://").then_some(url)
}

fn report_label(language: &str) -> &'static str {
    if language == "fa" {
        "📊 گزارش کامل check-host"
    } else {
        "📊 Full check-host report"
    }
}

async fn post(
    endpoint: &str,
    payload: &serde_json::Value,
    channel: &str,
    proxy: Option<&str>,
) -> bool {
    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15));

    if let Some(url) = proxy {
        match reqwest::Proxy::all(url) {
            Ok(proxy) => builder = builder.proxy(proxy),
            Err(err) => {
                eprintln!("the {channel} proxy {url} was rejected: {err}");
                return false;
            }
        }
    }

    let client = match builder.build() {
        Ok(client) => client,
        Err(err) => {
            eprintln!("could not build the {channel} client: {err}");
            return false;
        }
    };

    match client.post(endpoint).json(payload).send().await {
        Ok(response) if !response.status().is_success() => {
            eprintln!(
                "{channel} rejected the alert with HTTP {}",
                response.status()
            );
            false
        }
        Err(err) => {
            eprintln!("{channel} delivery failed: {err}");
            false
        }
        Ok(_) => true,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn alert() -> Alert {
        Alert {
            target: "softmac".into(),
            subject: Some("https://softmac.ir".into()),
            level: Level::Warn,
            findings: vec![Finding::warn(
                "website.abroadPartial",
                "1 of 6 outside vantage points failed: Sao Paulo (BR) <timeout> & retry",
            )],
            table: vec![
                cells(["FROM", "VANTAGE POINT", "PING", "RESULT"]),
                vec![
                    "LOCAL".into(),
                    "your network".into(),
                    "88 ms".into(),
                    "200".into(),
                ],
                vec![
                    "SA".into(),
                    "Sao Paulo (BR)".into(),
                    "-".into(),
                    "Connection timed out".into(),
                ],
            ],
            report: Some("https://check-host.net/check-report/abc".into()),
        }
    }

    #[test]
    fn the_columns_line_up() {
        assert_eq!(
            table(&alert().table),
            "FROM   VANTAGE POINT   PING   RESULT\n\
             LOCAL  your network    88 ms  200\n\
             SA     Sao Paulo (BR)  -      Connection timed out"
        );
    }

    #[test]
    fn telegram_markup_escapes_the_measurement_and_links_the_report() {
        let html = alert().html("fa");
        assert!(html.starts_with("<b>🟠 softmac — هشدار</b>"));
        assert!(html.contains("بعضی نودهای خارج دسترسی ندارند"));
        assert!(html.contains("&lt;timeout&gt; &amp; retry"));
        assert!(!html.contains("<timeout>"));
        assert!(html.contains("<pre>"));
        assert!(html.contains("<a href=\"https://check-host.net/check-report/abc\">"));
    }

    #[test]
    fn the_latin_measurement_never_shares_a_line_with_the_persian_headline() {
        for line in alert().text("fa", false).lines() {
            assert!(!(line.contains("هشدار") && line.contains("softmac.ir")));
        }
        assert!(alert().text("fa", false).contains("\n\u{2066}1 of 6"));
    }

    #[test]
    fn a_rich_message_uses_a_real_table() {
        let rich = alert().rich_html("fa");
        assert!(rich.starts_with("<h3>🟠 softmac — هشدار</h3>"));
        assert!(rich.contains("<table bordered striped compact>"));

        assert!(rich.contains("<tr><th>\u{2066}FROM\u{2069}</th>"));
        assert_eq!(rich.matches("<th>").count(), 4);
        assert_eq!(rich.matches("<tr>").count(), 3);
        assert!(rich.contains("&lt;timeout&gt; &amp; retry"));
        assert!(!rich.contains("<pre>"));
        assert!(rich.contains("<a href=\"https://check-host.net/check-report/abc\">"));
    }

    #[test]
    fn a_rich_report_link_that_is_not_https_never_becomes_an_href() {
        let mut alert = alert();
        alert.report = Some("javascript:alert(1)".into());
        assert!(!alert.rich_html("en").contains("href"));
    }

    #[test]
    fn latency_climbs_through_every_colour() {
        let grades: Vec<Grade> = [Some(120), Some(700), Some(1400), Some(3400), None]
            .into_iter()
            .map(grade_latency)
            .collect();
        assert_eq!(
            grades,
            [
                Grade::Best,
                Grade::Good,
                Grade::Fair,
                Grade::Poor,
                Grade::Pending
            ]
        );
        assert_eq!(grade_latency(Some(LATENCY_FAIR)), Grade::Poor);
        assert_eq!(grade_latency(Some(LATENCY_FAIR - 1)), Grade::Fair);
    }

    #[test]
    fn a_metric_is_graded_against_its_own_limit_not_a_fixed_number() {
        assert_eq!(grade_ratio(80.0, 85.0), Grade::Poor);
        assert_eq!(grade_ratio(80.0, 95.0), Grade::Fair);
        assert_eq!(grade_ratio(85.0, 85.0), Grade::Bad);
        assert_eq!(grade_ratio(1.4, 1.5), Grade::Poor);
        assert_eq!(grade_ratio(0.2, 1.5), Grade::Best);
        assert_eq!(grade_ratio(50.0, 0.0), Grade::Pending);
    }

    #[test]
    fn an_expiry_is_graded_against_the_warning_window() {
        assert_eq!(grade_days(-1, 14), Grade::Bad);
        assert_eq!(grade_days(10, 14), Grade::Poor);
        assert_eq!(grade_days(20, 14), Grade::Fair);
        assert_eq!(grade_days(50, 14), Grade::Good);
        assert_eq!(grade_days(300, 14), Grade::Best);
        assert_eq!(grade_days(1, 0), Grade::Poor);
    }

    #[test]
    fn a_reachable_node_that_answers_with_an_error_is_not_green() {
        let mut node = NodeResult {
            node: "de1".into(),
            country: "de".into(),
            city: "Frankfurt".into(),
            asn: "AS1".into(),
            reachable: Some(true),
            latency_ms: Some(90),
            status: Some("200".into()),
            message: None,
            address: None,
        };
        assert_eq!(grade_node(&node), Grade::Best);
        node.status = Some("503".into());
        assert_eq!(grade_node(&node), Grade::Poor);
        node.reachable = Some(false);
        assert_eq!(grade_node(&node), Grade::Bad);
        node.reachable = None;
        assert_eq!(grade_node(&node), Grade::Pending);
    }

    #[test]
    fn a_grade_dot_does_not_shift_the_monospace_columns() {
        let block = table(&[
            cells(["ORIGIN", "RESULT"]),
            vec![graded(Grade::Best, "IR"), "200".into()],
            vec![graded(Grade::Bad, "SA"), "timed out".into()],
        ]);
        let starts: Vec<usize> = block
            .lines()
            .map(|line| width(line.rsplit_once("  ").unwrap().0) + 2)
            .collect();
        assert_eq!(starts, [8, 8, 8]);
    }

    #[test]
    fn a_persian_heading_is_not_pinned_left_to_right() {
        assert_eq!(isolate("نتیجه"), "نتیجه");
        assert_eq!(isolate("200 OK"), "\u{2066}200 OK\u{2069}");
        assert_eq!(isolate(""), "");
    }

    #[test]
    fn an_empty_cell_stays_empty_rather_than_carrying_bidi_marks() {
        let mut alert = alert();
        alert.table.push(vec![
            "box nginx".into(),
            "running".into(),
            String::new(),
            String::new(),
        ]);
        assert!(alert.rich_html("en").contains("<td></td><td></td>"));
    }

    #[test]
    fn a_report_link_that_is_not_https_never_becomes_an_href() {
        let mut alert = alert();
        alert.report = Some("javascript:alert(1)".into());
        assert!(!alert.html("en").contains("href"));
    }

    #[test]
    fn a_lone_row_is_left_to_the_finding_detail() {
        let mut alert = alert();
        alert.table.truncate(2);
        assert!(alert.block().is_none());
    }

    #[test]
    fn an_oversized_alert_still_fits_the_telegram_limit() {
        let mut alert = alert();

        alert.findings = (0..MAX_LINES)
            .map(|_| Finding::warn("docker.notRunning", "&".repeat(4000)))
            .collect();
        alert.table = (0..60)
            .map(|_| cells(["AS", "Somewhere (XX)", "-", &"y".repeat(400)]))
            .collect();
        let (text, markup) = alert.telegram_payload("fa");
        assert!(text.chars().count() <= TELEGRAM_LIMIT);
        assert!(!markup, "truncated HTML would be rejected by Telegram");
    }

    #[test]
    fn a_realistic_alert_keeps_its_markup() {
        let mut alert = alert();
        alert.table = std::iter::once(cells(["FROM", "VANTAGE POINT", "PING", "RESULT"]))
            .chain((0..MAX_ROWS).map(|_| {
                cells([
                    "EU",
                    "Nyiregyhaza (HU)",
                    "1204 ms",
                    "Connection timed out after 10000 ms",
                ])
            }))
            .collect();
        let (text, markup) = alert.telegram_payload("fa");
        assert!(markup, "{} chars was too long", text.chars().count());
        assert!(text.contains("<pre>"));
    }
}
