//! State owned by one language-server session.
use super::*;

pub(super) struct Document {
    pub(super) source_id: SourceId,
    pub(super) text: String,
    pub(super) analysis: Result<DocumentAnalysis, FrontendError>,
}

pub(super) struct Server {
    pub(super) connection: Connection,
    pub(super) documents: HashMap<String, Document>,
    pub(super) workspace_documents: HashSet<String>,
    pub(super) host_contract: HostContract,
    pub(super) host_functions: HashMap<String, FunctionSignature>,
    pub(super) host_types: HashSet<String>,
    pub(super) projects: Vec<Project>,
    pub(super) compilation: CompilationSession,
    pub(super) next_source_id: u32,
}
