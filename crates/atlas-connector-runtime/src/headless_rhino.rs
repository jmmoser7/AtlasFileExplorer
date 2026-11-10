//! Closed Rhino SDK jobs, called on a worker after existing Session::begin admission.
//! No connector transport, surface, source write-back or preview renderer is replaced.
mod plan;
mod plan_contract;
mod session;
mod worker;
pub use plan::{GeometryOperation, GeometryPlan, Units};
pub use session::HeadlessDraft;
pub use worker::{GeneratedModel, HeadlessRhino, ModelReport};

pub const GENERATE_COMMAND: &str = "org.slate.rhino.headless.generate";

/// Existing connector admission checks draft authority before dispatch. Detailed
/// closed-plan validation follows deserialization, before any process is started.
pub fn command_descriptor() -> atlas_connector::CommandDescriptor {
    atlas_connector::CommandDescriptor {
        id: GENERATE_COMMAND.into(),
        title: "Generate isolated Rhino model".into(),
        effect: atlas_connector::CommandEffect::DraftMutation,
        undo: atlas_connector::UndoOwner::NativeApplication,
        cancellable: true,
        agent_allowed: true,
        parameters: serde_json::json!({
            "type":"object", "additionalProperties":false,
            "required":["plan"],
            "properties":{
                "plan":{
                    "type":"string","minLength":1,"maxLength":24576,
                    "contentMediaType":"application/json",
                    "description":plan_contract::description()
                }
            }
        }),
    }
}

/// The existing v0 parameter dialect is scalar-only. JSON here is declarative
/// typed data, never code; unknown operations/fields are rejected by serde.
pub fn parse_parameters(parameters: &serde_json::Value) -> Result<GeometryPlan, String> {
    atlas_connector::validate_parameters(&command_descriptor().parameters, parameters)
        .map_err(|e| e.to_string())?;
    let plan: GeometryPlan =
        serde_json::from_str(parameters["plan"].as_str().ok_or("Missing plan")?)
            .map_err(|e| format!("Unsupported/invalid geometry plan: {e}"))?;
    plan.validate()?;
    Ok(plan)
}
