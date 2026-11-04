use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Request {
    GetRuntimeStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub protocol_version: u16,
    pub app_version: String,
    pub platform: String,
    pub integration: IntegrationStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationStage {
    Scaffold,
    WindowFrames,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_a_stable_tagged_wire_shape() {
        let json = serde_json::to_string(&Request::GetRuntimeStatus).unwrap();
        assert_eq!(json, r#"{"type":"get_runtime_status"}"#);

        let decoded: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, Request::GetRuntimeStatus);
    }

    #[test]
    fn runtime_status_uses_frontend_field_names() {
        let status = RuntimeStatus {
            protocol_version: PROTOCOL_VERSION,
            app_version: "0.1.0".to_owned(),
            platform: "windows".to_owned(),
            integration: IntegrationStage::Scaffold,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains(r#""appVersion":"0.1.0""#));
        assert!(json.contains(r#""protocolVersion":1"#));
    }

    #[test]
    fn integration_stage_names_are_stable() {
        let json = serde_json::to_string(&IntegrationStage::WindowFrames).unwrap();
        assert_eq!(json, r#""window_frames""#);
    }
}
