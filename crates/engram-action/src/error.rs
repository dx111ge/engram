/// Action engine errors.

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("rule parse error: {0}")]
    RuleParse(String),
    #[error("condition evaluation error: {0}")]
    Condition(String),
    #[error("effect execution error: {0}")]
    Effect(String),
    #[error("safety constraint violated: {0}")]
    Safety(String),
    #[error("graph error: {0}")]
    Graph(String),
    #[error("webhook error: {0}")]
    Webhook(String),
    #[error("timer error: {0}")]
    Timer(String),
}
