pub mod commit;
pub mod fan_out;
pub mod final_gates;
pub mod implement;
pub mod integrate;
pub mod join;
pub mod plan;
pub mod prepare;
pub mod repair;
pub mod review;
pub mod support;
pub mod test;

use heikas_domain::graph::NodeId;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::{ApplicationError, ApplicationResult};

pub async fn execute(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    match context.node {
        NodeId::Prepare => prepare::execute(context).await,
        NodeId::Plan => plan::execute(context).await,
        NodeId::FanOut => fan_out::execute(context).await,
        NodeId::ImplementCandidate => implement::execute(context).await,
        NodeId::TestCandidate => test::execute(context).await,
        NodeId::ReviewCandidate => review::execute(context).await,
        NodeId::RepairCandidate => repair::execute(context).await,
        NodeId::Join => join::execute(context).await,
        NodeId::IntegrateWinner => integrate::execute(context).await,
        NodeId::FinalTest => final_gates::final_test(context).await,
        NodeId::FinalReview => final_gates::final_review(context).await,
        NodeId::Commit => commit::execute(context).await,
        NodeId::Approval | NodeId::CommitApproval => Err(ApplicationError::Internal(format!(
            "node {} is resolved by the scheduler and is never dispatched",
            context.node.as_str()
        ))),
    }
}
