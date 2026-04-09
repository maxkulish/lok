#[allow(dead_code)]
mod diff_applier;
mod edit_parser;
#[allow(dead_code)]
mod retry_loop;
#[allow(dead_code)]
mod rollback;
#[allow(dead_code)]
mod verification;

#[allow(unused_imports)]
pub use diff_applier::{ApplyError, ApplyErrorKind, ApplyResult, DiffApplier, FileBackup};
#[allow(unused_imports)]
pub use edit_parser::{EditFormat, EditParseError, EditParser, ParsedEdits};
#[allow(unused_imports)]
pub use retry_loop::{
    AttemptRecord, EditRequester, RetryContext, RetryLoop, RetryLoopOutcome, RetryReason,
};
#[allow(unused_imports)]
pub use rollback::{Rollback, RollbackFailure, RollbackReport};
#[allow(unused_imports)]
pub use verification::{Verification, VerifyResult};
