//! Windows platform implementations using PowerShell and COM automation

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::process::Command;
use tracing::{debug, warn};

use super::{CalendarProvider, EmailProvider, UiAutomation};

/// Sanitize a string for safe use in PowerShell
/// Escapes backticks, dollar signs, double/single quotes, and control characters
fn sanitize_powershell_string(input: &str) -> String {
    input
        .replace('`', "``")
        .replace('$', "`$")
        .replace('"', "`\"")
        .replace('\'', "''")
        .replace('\n', "`n")
        .replace('\r', "`r")
        .chars()
        .filter(|&c| c >= ' ' || c == '\t')
        .collect()
}

/// Sanitize text for use with [System.Windows.Forms.SendKeys]::SendWait()
/// SendKeys treats {, }, +, ^, %, ~ as special characters
fn sanitize_sendkeys_string(input: &str) -> String {
    let mut result = String::with_capacity(input.len() * 2);
    for c in input.chars() {
        match c {
            '{' => result.push_str("{{}"),
            '}' => result.push_str("{}}"),
            '+' => result.push_str("{+}"),
            '^' => result.push_str("{^}"),
            '%' => result.push_str("{%}"),
            '~' => result.push_str("{~}"),
            '(' => result.push_str("{(}"),
            ')' => result.push_str("{)}"),
            _ => result.push(c),
        }
    }
    result
}

/// Run a PowerShell script with 30 second timeout
async fn run_powershell(script: &str) -> Result<String> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("PowerShell execution timed out after 30 seconds"))?
    .context("Failed to execute PowerShell")?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let error = String::from_utf8_lossy(&output.stderr).to_string();
        warn!("PowerShell failed: {}", error);
        Err(anyhow::anyhow!("PowerShell failed: {}", error))
    }
}

pub struct WindowsEmailProvider;

#[async_trait]
impl EmailProvider for WindowsEmailProvider {
    async fn read_emails(&self, limit: u64, mailbox: &str, search: Option<&str>) -> Result<String> {
        debug!("Reading {} emails from Outlook ({})", limit, mailbox);
        let folder = match mailbox.to_lowercase().as_str() {
            "inbox" => "6",
            "sent" => "5",
            "drafts" => "16",
            "trash" => "3",
            _ => "6",
        };
        let filter_clause = if let Some(term) = search {
            let safe_term = sanitize_powershell_string(term);
            format!(
                r#"$items = $items | Where-Object {{ $_.Subject -like "*{}*" -or $_.SenderName -like "*{}*" }}"#,
                safe_term, safe_term
            )
        } else {
            String::new()
        };
        let script = format!(
            r#"
try {{
    $outlook = New-Object -ComObject Outlook.Application
    $namespace = $outlook.GetNamespace("MAPI")
    $folder = $namespace.GetDefaultFolder({folder})
    $items = $folder.Items
    $items.Sort("[ReceivedTime]", $true)
    {filter_clause}
    $count = [Math]::Min($items.Count, {limit})
    $output = ""
    for ($i = 1; $i -le $count; $i++) {{
        $msg = $items.Item($i)
        $body = $msg.Body
        if ($body.Length -gt 500) {{ $body = $body.Substring(0, 500) }}
        $output += "From: $($msg.SenderName) <$($msg.SenderEmailAddress)>`n"
        $output += "Subject: $($msg.Subject)`n"
        $output += "Date: $($msg.ReceivedTime)`n"
        $output += "Preview: $body`n"
        $output += "---`n"
    }}
    Write-Output $output
}} catch {{
    Write-Error "Error reading emails: $_"
}}
"#
        );
        run_powershell(&script).await
    }

    async fn send_email(
        &self,
        to: &str,
        subject: &str,
        body: &str,
        cc: Option<&str>,
        in_reply_to: Option<&str>,
    ) -> Result<String> {
        let safe_to = sanitize_powershell_string(to);
        let safe_subject = sanitize_powershell_string(subject);
        let safe_body = sanitize_powershell_string(body);
        let script = if let Some(reply_subject) = in_reply_to {
            let safe_reply = sanitize_powershell_string(reply_subject);
            debug!("Replying to email with subject: {}", reply_subject);
            format!(
                r#"
try {{
    $outlook = New-Object -ComObject Outlook.Application
    $namespace = $outlook.GetNamespace("MAPI")
    $inbox = $namespace.GetDefaultFolder(6)
    $items = $inbox.Items
    $found = $items.Find("[Subject] = '{safe_reply}'")
    if ($found -ne $null) {{
        $reply = $found.Reply()
        $reply.Body = "{safe_body}" + "`n`n" + $reply.Body
        $reply.Send()
        Write-Output "Reply sent (threaded)"
    }} else {{
        $mail = $outlook.CreateItem(0)
        $mail.To = "{safe_to}"
        $mail.Subject = "{safe_subject}"
        $mail.Body = "{safe_body}"
        $mail.Send()
        Write-Output "Email sent (no original found for threading)"
    }}
}} catch {{
    Write-Error "Error sending email: $_"
}}
"#
            )
        } else {
            debug!("Sending new email to: {}", to);
            let cc_line = if let Some(cc_addr) = cc {
                let safe_cc = sanitize_powershell_string(cc_addr);
                format!("    $mail.CC = \"{safe_cc}\"")
            } else {
                String::new()
            };
            format!(
                r#"
try {{
    $outlook = New-Object -ComObject Outlook.Application
    $mail = $outlook.CreateItem(0)
    $mail.To = "{safe_to}"
    $mail.Subject = "{safe_subject}"
    $mail.Body = "{safe_body}"
{cc_line}
    $mail.Send()
    Write-Output "Email sent successfully"
}} catch {{
    Write-Error "Error sending email: $_"
}}
"#
            )
        };
        run_powershell(&script).await
    }
}

pub struct WindowsCalendarProvider;

#[async_trait]
impl CalendarProvider for WindowsCalendarProvider {
    async fn read_events(&self, days_ahead: u64) -> Result<String> {
        debug!(
            "Reading calendar events for next {} days from Outlook",
            days_ahead
        );
        let script = format!(
            r#"
try {{
    $outlook = New-Object -ComObject Outlook.Application
    $namespace = $outlook.GetNamespace("MAPI")
    $calendar = $namespace.GetDefaultFolder(9)
    $items = $calendar.Items
    $items.IncludeRecurrences = $true
    $items.Sort("[Start]")
    $start = (Get-Date).ToString("g")
    $end = (Get-Date).AddDays({days_ahead}).ToString("g")
    $restrict = "[Start] >= '$start' AND [Start] <= '$end'"
    $filtered = $items.Restrict($restrict)
    $output = ""
    foreach ($evt in $filtered) {{
        $output += "Event: $($evt.Subject)`n"
        $output += "Start: $($evt.Start)`n"
        $output += "End: $($evt.End)`n"
        $output += "---`n"
    }}
    Write-Output $output
}} catch {{
    Write-Error "Error reading calendar: $_"
}}
"#
        );
        run_powershell(&script).await
    }

    async fn create_event(
        &self,
        summary: &str,
        start_time: &str,
        duration_minutes: u64,
    ) -> Result<String> {
        debug!("Creating calendar event: {}", summary);
        let safe_summary = sanitize_powershell_string(summary);
        let safe_start = sanitize_powershell_string(start_time);
        let script = format!(
            r#"
try {{
    $outlook = New-Object -ComObject Outlook.Application
    $appt = $outlook.CreateItem(1)
    $appt.Subject = "{safe_summary}"
    $appt.Start = [DateTime]::Parse("{safe_start}")
    $appt.Duration = {duration_minutes}
    $appt.Save()
    Write-Output "Event created successfully"
}} catch {{
    Write-Error "Error creating event: $_"
}}
"#
        );
        run_powershell(&script).await
    }
}

pub struct WindowsUiAutomation;

#[async_trait]
impl UiAutomation for WindowsUiAutomation {
    async fn read_screen(&self) -> Result<String> {
        debug!("Reading screen information via UI Automation");
        let script = r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
try {
    $root = [System.Windows.Automation.AutomationElement]::FocusedElement
    $process = Get-Process -Id $root.Current.ProcessId -ErrorAction SilentlyContinue
    $appName = if ($process) { $process.MainWindowTitle } else { $root.Current.Name }
    $processName = if ($process) { $process.ProcessName } else { "unknown" }
    Write-Output "App: $processName`nWindow: $appName"
} catch {
    Write-Error "Error reading screen: $_"
}
"#;
        run_powershell(script).await
    }

    async fn click_element(&self, element_name: &str, element_type: &str) -> Result<String> {
        debug!("Clicking {} element: {}", element_type, element_name);
        let safe_name = sanitize_powershell_string(element_name);
        let _ = element_type; // Windows UI Automation searches by name, not type
        let script = format!(
            r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
try {{
    $root = [System.Windows.Automation.AutomationElement]::FocusedElement
    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty, "{safe_name}")
    $element = $root.FindFirst([System.Windows.Automation.TreeScope]::Subtree, $condition)
    if ($element -ne $null) {{
        $invokePattern = $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $invokePattern.Invoke()
        Write-Output "Clicked successfully"
    }} else {{
        Write-Error "Element '{safe_name}' not found"
    }}
}} catch {{
    Write-Error "Error clicking element: $_"
}}
"#
        );
        run_powershell(&script).await
    }

    async fn type_text(&self, text: &str) -> Result<String> {
        debug!("Typing text ({} chars)", text.len());
        // First escape SendKeys meta-characters, then escape for PowerShell string embedding
        let sendkeys_safe = sanitize_sendkeys_string(text);
        let safe_text = sanitize_powershell_string(&sendkeys_safe);
        let script = format!(
            r#"
Add-Type -AssemblyName System.Windows.Forms
try {{
    [System.Windows.Forms.SendKeys]::SendWait("{safe_text}")
    Write-Output "Text typed successfully"
}} catch {{
    Write-Error "Error typing text: $_"
}}
"#
        );
        run_powershell(&script).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_powershell_string() {
        assert_eq!(sanitize_powershell_string("test`back"), "test``back");
        assert_eq!(sanitize_powershell_string("test$var"), "test`$var");
        assert_eq!(sanitize_powershell_string("test\"quote"), "test`\"quote");
        assert_eq!(sanitize_powershell_string("test\nline"), "test`nline");
    }

    #[test]
    fn test_sanitize_single_quotes() {
        let attack = "'; Remove-Item C:\\ -Recurse -Force; '";
        let safe = sanitize_powershell_string(attack);
        assert!(safe.contains("''"));
        assert!(!safe.contains("'; "));
    }

    #[test]
    fn test_sanitize_prevents_injection() {
        let attack = "test\"; Remove-Item -Recurse -Force C:\\; \"";
        let safe = sanitize_powershell_string(attack);
        assert!(safe.contains("`\""));
        assert!(!safe.contains("\";"));
    }

    #[test]
    fn test_sanitize_sendkeys_string() {
        // SendKeys meta-characters should be wrapped in braces
        assert_eq!(sanitize_sendkeys_string("{hello}"), "{{}hello{}}");
        assert_eq!(sanitize_sendkeys_string("^c"), "{^}c");
        assert_eq!(sanitize_sendkeys_string("%{F4}"), "{%}{{}F4{}}");
        assert_eq!(sanitize_sendkeys_string("normal text"), "normal text");
        assert_eq!(sanitize_sendkeys_string("a+b"), "a{+}b");
        assert_eq!(sanitize_sendkeys_string("~"), "{~}");
    }
}
