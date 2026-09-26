/// nova-provider-aws — gRPC stub server
///
/// Startup protocol:
///   1. Bind a random local TCP port
///   2. Print one JSON line to stdout: {"addr":"127.0.0.1:<port>","protocol":"grpc","protocol_version":1}
///   3. Serve the ProviderService gRPC interface until killed

pub mod proto {
    tonic::include_proto!("nova.provider.v1");
}

use proto::{
    provider_service_server::{ProviderService, ProviderServiceServer},
    ApplyRequest, ApplyResponse, AttributeSchema, ConfigureRequest, ConfigureResponse,
    DestroyRequest, DestroyResponse, Empty, PlanRequest, PlanResponse, ProviderSchema,
    ReadRequest, ReadResponse, ResourceDiff, ResourceState, ResourceTypeSchema,
};
use std::collections::HashMap;
use tonic::{transport::Server, Request, Response, Status};

// ─────────────────────────────────────────────────────────────────────────────
// Provider implementation
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct AwsProvider;

#[tonic::async_trait]
impl ProviderService for AwsProvider {
    async fn get_schema(&self, _: Request<Empty>) -> Result<Response<ProviderSchema>, Status> {
        let schema = ProviderSchema {
            provider_name: "aws".into(),
            provider_version: "0.1.0".into(),
            config_schema: vec![
                attr("region", "string", true, false, "AWS region (e.g. us-east-1)"),
                attr("access_key", "string", false, false, "AWS access key ID"),
                attr("secret_key", "string", false, false, "AWS secret access key"),
            ],
            resource_types: vec![
                ResourceTypeSchema {
                    type_name: "aws_s3_bucket".into(),
                    description: "An S3 bucket".into(),
                    attributes: vec![
                        attr("name", "string", true, false, "Bucket name"),
                        attr("arn", "string", false, true, "Bucket ARN (computed)"),
                        attr("region", "string", false, true, "Bucket region (computed)"),
                    ],
                },
                ResourceTypeSchema {
                    type_name: "aws_iam_role".into(),
                    description: "An IAM role".into(),
                    attributes: vec![
                        attr("name", "string", true, false, "Role name"),
                        attr("assume_role_policy", "string", true, false, "Trust policy JSON"),
                        attr("arn", "string", false, true, "Role ARN (computed)"),
                    ],
                },
            ],
        };
        Ok(Response::new(schema))
    }

    async fn configure(
        &self,
        request: Request<ConfigureRequest>,
    ) -> Result<Response<ConfigureResponse>, Status> {
        let config = request.into_inner().config;
        if !config.contains_key("region") {
            return Ok(Response::new(ConfigureResponse {
                ok: false,
                error: "Missing required field: region".into(),
            }));
        }
        eprintln!(
            "[aws] Configured with region={}",
            config.get("region").unwrap()
        );
        Ok(Response::new(ConfigureResponse { ok: true, error: String::new() }))
    }

    async fn plan_resource(
        &self,
        request: Request<PlanRequest>,
    ) -> Result<Response<PlanResponse>, Status> {
        let req = request.into_inner();
        eprintln!("[aws] PlanResource: {}.{}", req.resource_type, req.resource_name);

        // Stub: everything is a CREATE (no state exists yet)
        let desired = req.config.clone();

        // Simulate computed fields that the provider would fill in
        let changed_keys: Vec<String> = desired.keys().cloned().collect();

        let diff = ResourceDiff {
            resource_type: req.resource_type,
            resource_name: req.resource_name,
            action: 1, // ACTION_CREATE
            desired,
            current: HashMap::new(),
            changed_keys,
        };

        Ok(Response::new(PlanResponse {
            diff: Some(diff),
            error: String::new(),
        }))
    }

    async fn apply_resource(
        &self,
        request: Request<ApplyRequest>,
    ) -> Result<Response<ApplyResponse>, Status> {
        let diff = request
            .into_inner()
            .diff
            .ok_or_else(|| Status::invalid_argument("Missing diff in ApplyRequest"))?;

        eprintln!("[aws] ApplyResource: {}.{}", diff.resource_type, diff.resource_name);

        // Stub: echo desired state back as the resulting state
        let mut attributes = diff.desired.clone();

        // Simulate computed fields
        match diff.resource_type.as_str() {
            "aws_s3_bucket" => {
                let name = attributes.get("name").cloned().unwrap_or_default();
                attributes.insert("arn".into(), format!("arn:aws:s3:::{name}"));
                attributes.insert("region".into(), "us-east-1".into());
            }
            "aws_iam_role" => {
                let name = attributes.get("name").cloned().unwrap_or_default();
                attributes.insert(
                    "arn".into(),
                    format!("arn:aws:iam::123456789012:role/{name}"),
                );
            }
            _ => {}
        }

        let state = ResourceState {
            resource_type: diff.resource_type,
            resource_name: diff.resource_name.clone(),
            id: diff.resource_name,
            attributes,
        };

        Ok(Response::new(ApplyResponse { state: Some(state), error: String::new() }))
    }

    async fn destroy_resource(
        &self,
        request: Request<DestroyRequest>,
    ) -> Result<Response<DestroyResponse>, Status> {
        let state = request.into_inner().state.unwrap_or_default();
        eprintln!("[aws] DestroyResource: {}.{}", state.resource_type, state.resource_name);
        Ok(Response::new(DestroyResponse { ok: true, error: String::new() }))
    }

    async fn read_resource(
        &self,
        request: Request<ReadRequest>,
    ) -> Result<Response<ReadResponse>, Status> {
        let req = request.into_inner();
        eprintln!("[aws] ReadResource: {} id={}", req.resource_type, req.id);
        // Stub: return empty state (resource not found)
        Ok(Response::new(ReadResponse {
            state: Some(ResourceState {
                resource_type: req.resource_type,
                resource_name: String::new(),
                id: req.id,
                attributes: HashMap::new(),
            }),
            error: String::new(),
        }))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Main: bind → print handshake → serve
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Bind on a random available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    // Print the handshake JSON — nova reads this line to know where to connect
    // IMPORTANT: this must be the very first line printed to stdout
    println!(
        r#"{{"addr":"{}","protocol":"grpc","protocol_version":1}}"#,
        addr
    );

    eprintln!("[aws] gRPC server listening on {addr}");

    // Convert TcpListener → tokio-stream for tonic
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    Server::builder()
        .add_service(ProviderServiceServer::new(AwsProvider))
        .serve_with_incoming(incoming)
        .await?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn attr(name: &str, r#type: &str, required: bool, computed: bool, description: &str) -> AttributeSchema {
    AttributeSchema {
        name: name.into(),
        r#type: r#type.into(),
        required,
        computed,
        description: description.into(),
    }
}
