use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Log level for debug messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "TRACE"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO "),
            LogLevel::Warn => write!(f, "WARN "),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

/// Log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: Instant,
    pub level: LogLevel,
    pub component: String,
    pub message: String,
    pub worker_id: Option<usize>,
    pub task_id: Option<usize>,
}

impl fmt::Display for LogEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let elapsed = self.timestamp.elapsed();
        write!(
            f,
            "[{:>5}.{:03}] [{}]",
            elapsed.as_secs(),
            elapsed.subsec_millis(),
            self.level
        )?;

        if let Some(worker) = self.worker_id {
            write!(f, " [W{}]", worker)?;
        }

        if let Some(task) = self.task_id {
            write!(f, " [T{}]", task)?;
        }

        write!(f, " [{}] {}", self.component, self.message)
    }
}

/// Debug logger for taskflow execution
pub struct DebugLogger {
    enabled: Arc<Mutex<bool>>,
    log_level: Arc<Mutex<LogLevel>>,
    logs: Arc<Mutex<Vec<LogEntry>>>,
    start_time: Instant,
}

impl DebugLogger {
    /// Create a new debug logger
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(Mutex::new(false)),
            log_level: Arc::new(Mutex::new(LogLevel::Info)),
            logs: Arc::new(Mutex::new(Vec::new())),
            start_time: Instant::now(),
        }
    }

    /// Enable debug logging
    pub fn enable(&self) {
        *self.enabled.lock().unwrap() = true;
    }

    /// Disable debug logging
    pub fn disable(&self) {
        *self.enabled.lock().unwrap() = false;
    }

    /// Set log level
    pub fn set_log_level(&self, level: LogLevel) {
        *self.log_level.lock().unwrap() = level;
    }

    /// Check if logging is enabled
    pub fn is_enabled(&self) -> bool {
        *self.enabled.lock().unwrap()
    }

    /// Log a message
    pub fn log(
        &self,
        level: LogLevel,
        component: &str,
        message: &str,
        worker_id: Option<usize>,
        task_id: Option<usize>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let current_level = *self.log_level.lock().unwrap();
        if level < current_level {
            return;
        }

        let entry = LogEntry {
            timestamp: self.start_time,
            level,
            component: component.to_string(),
            message: message.to_string(),
            worker_id,
            task_id,
        };

        // Print to console
        println!("{}", entry);

        // Store in buffer
        self.logs.lock().unwrap().push(entry);
    }

    /// Convenience method for trace level
    pub fn trace(&self, component: &str, message: &str) {
        self.log(LogLevel::Trace, component, message, None, None);
    }

    /// Convenience method for debug level
    pub fn debug(&self, component: &str, message: &str) {
        self.log(LogLevel::Debug, component, message, None, None);
    }

    /// Convenience method for info level
    pub fn info(&self, component: &str, message: &str) {
        self.log(LogLevel::Info, component, message, None, None);
    }

    /// Convenience method for warn level
    pub fn warn(&self, component: &str, message: &str) {
        self.log(LogLevel::Warn, component, message, None, None);
    }

    /// Convenience method for error level
    pub fn error(&self, component: &str, message: &str) {
        self.log(LogLevel::Error, component, message, None, None);
    }

    /// Get all log entries
    pub fn get_logs(&self) -> Vec<LogEntry> {
        self.logs.lock().unwrap().clone()
    }

    /// Clear all logs
    pub fn clear(&self) {
        self.logs.lock().unwrap().clear();
    }

    /// Export logs to string
    pub fn export_logs(&self) -> String {
        let logs = self.logs.lock().unwrap();
        logs.iter().map(|entry| format!("{}\n", entry)).collect()
    }

    /// Save logs to file
    pub fn save_to_file(&self, filename: &str) -> std::io::Result<()> {
        use std::fs::File;
        use std::io::Write;

        let mut file = File::create(filename)?;
        file.write_all(self.export_logs().as_bytes())?;
        Ok(())
    }
}

impl Clone for DebugLogger {
    fn clone(&self) -> Self {
        Self {
            enabled: Arc::clone(&self.enabled),
            log_level: Arc::clone(&self.log_level),
            logs: Arc::clone(&self.logs),
            start_time: self.start_time,
        }
    }
}

impl Default for DebugLogger {
    fn default() -> Self {
        Self::new()
    }
}

/// Macro for convenient logging
#[macro_export]
macro_rules! log_debug {
    ($logger:expr, $level:expr, $component:expr, $($arg:tt)*) => {
        $logger.log($level, $component, &format!($($arg)*), None, None)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger_creation() {
        let logger = DebugLogger::new();
        assert!(!logger.is_enabled());
    }

    #[test]
    fn test_logging() {
        let logger = DebugLogger::new();
        logger.enable();
        logger.set_log_level(LogLevel::Debug);

        logger.info("test", "This is a test");
        logger.debug("test", "Debug message");

        let logs = logger.get_logs();
        assert_eq!(logs.len(), 2);
    }

    #[test]
    fn test_log_filtering() {
        let logger = DebugLogger::new();
        logger.enable();
        logger.set_log_level(LogLevel::Warn);

        logger.debug("test", "Should not appear");
        logger.info("test", "Should not appear");
        logger.warn("test", "Should appear");
        logger.error("test", "Should appear");

        let logs = logger.get_logs();
        assert_eq!(logs.len(), 2);
    }
}
