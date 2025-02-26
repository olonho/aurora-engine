use crate::admin_controlled::PausedMask;
use crate::fungible_token::FungibleTokenMetadata;
use crate::json::{JsonError, JsonValue};
use crate::prelude::account_id::AccountId;
use crate::prelude::{
    format, Balance, BorshDeserialize, BorshSerialize, EthAddress, RawAddress, RawH256, RawU256,
    String, ToString, TryFrom, Vec, WeiU256,
};
use crate::proof::Proof;
use aurora_engine_types::types::Fee;
use evm::backend::Log;

/// Borsh-encoded parameters for the `new` function.
#[derive(BorshSerialize, BorshDeserialize)]
pub struct NewCallArgs {
    /// Chain id, according to the EIP-115 / ethereum-lists spec.
    pub chain_id: RawU256,
    /// Account which can upgrade this contract.
    /// Use empty to disable updatability.
    pub owner_id: AccountId,
    /// Account of the bridge prover.
    /// Use empty to not use base token as bridged asset.
    pub bridge_prover_id: AccountId,
    /// How many blocks after staging upgrade can deploy it.
    pub upgrade_delay_blocks: u64,
}

/// Borsh-encoded parameters for the `meta_call` function.
#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct MetaCallArgs {
    pub signature: [u8; 64],
    pub v: u8,
    pub nonce: RawU256,
    pub fee_amount: RawU256,
    pub fee_address: RawAddress,
    pub contract_address: RawAddress,
    pub value: RawU256,
    pub method_def: String,
    pub args: Vec<u8>,
}

/// Borsh-encoded log for use in a `SubmitResult`.
#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct ResultLog {
    pub address: RawAddress,
    pub topics: Vec<RawU256>,
    pub data: Vec<u8>,
}

impl From<Log> for ResultLog {
    fn from(log: Log) -> Self {
        let topics = log
            .topics
            .into_iter()
            .map(|topic| topic.0)
            .collect::<Vec<_>>();
        ResultLog {
            address: log.address.0,
            topics,
            data: log.data,
        }
    }
}

/// The status of a transaction.
#[derive(Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub enum TransactionStatus {
    Succeed(Vec<u8>),
    Revert(Vec<u8>),
    OutOfGas,
    OutOfFund,
    OutOfOffset,
    CallTooDeep,
}

impl TransactionStatus {
    pub fn is_ok(&self) -> bool {
        matches!(*self, TransactionStatus::Succeed(_))
    }

    pub fn is_revert(&self) -> bool {
        matches!(*self, TransactionStatus::Revert(_))
    }

    pub fn is_fail(&self) -> bool {
        *self == TransactionStatus::OutOfGas
            || *self == TransactionStatus::OutOfFund
            || *self == TransactionStatus::OutOfOffset
            || *self == TransactionStatus::CallTooDeep
    }
}

impl AsRef<[u8]> for TransactionStatus {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Succeed(_) => b"SUCCESS",
            Self::Revert(_) => b"ERR_REVERT",
            Self::OutOfFund => b"ERR_OUT_OF_FUNDS",
            Self::OutOfGas => b"ERR_OUT_OF_GAS",
            Self::OutOfOffset => b"ERR_OUT_OF_OFFSET",
            Self::CallTooDeep => b"ERR_CALL_TOO_DEEP",
        }
    }
}

/// Borsh-encoded parameters for the `call`, `call_with_args`, `deploy_code`,
/// and `deploy_with_input` methods.
#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct SubmitResult {
    version: u8,
    pub status: TransactionStatus,
    pub gas_used: u64,
    pub logs: Vec<ResultLog>,
}

impl SubmitResult {
    /// Must be incremented when making breaking changes to the SubmitResult ABI.
    /// The current value of 7 is chosen because previously a `TransactionStatus` object
    /// was first in the serialization, which is an enum with less than 7 variants.
    /// Therefore, no previous `SubmitResult` would have began with a leading 7 byte,
    /// and this can be used to distinguish the new ABI (with version byte) from the old.
    const VERSION: u8 = 7;

    pub fn new(status: TransactionStatus, gas_used: u64, logs: Vec<ResultLog>) -> Self {
        Self {
            version: Self::VERSION,
            status,
            gas_used,
            logs,
        }
    }
}

/// Borsh-encoded parameters for the engine `call` function.
#[derive(BorshSerialize, BorshDeserialize, Debug, PartialEq, Eq, Clone)]
pub struct FunctionCallArgsV2 {
    pub contract: RawAddress,
    /// Wei compatible Borsh-encoded value field to attach an ETH balance to the transaction
    pub value: WeiU256,
    pub input: Vec<u8>,
}

/// Legacy Borsh-encoded parameters for the engine `call` function, to provide backward type compatibility
#[derive(BorshSerialize, BorshDeserialize, Debug, PartialEq, Eq, Clone)]
pub struct FunctionCallArgsV1 {
    pub contract: RawAddress,
    pub input: Vec<u8>,
}

/// Deserialized values from bytes to current or legacy Borsh-encoded parameters
/// for passing to the engine `call` function, and to provide backward type compatibility
#[derive(BorshSerialize, BorshDeserialize, Debug, PartialEq, Eq, Clone)]
pub enum CallArgs {
    V2(FunctionCallArgsV2),
    V1(FunctionCallArgsV1),
}

impl CallArgs {
    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        // For handling new input format (wrapped into call args enum) - for data structures with new arguments,
        // made for flexibility and extensibility.
        if let Ok(value) = Self::try_from_slice(bytes) {
            Some(value)
            // Fallback, for handling old input format,
            // i.e. input, formed as a raw (not wrapped into call args enum) data structure with legacy arguments,
            // made for backward compatibility.
        } else if let Ok(value) = FunctionCallArgsV1::try_from_slice(bytes) {
            Some(Self::V1(value))
            // Dealing with unrecognized input should be handled and result as an exception in a call site.
        } else {
            None
        }
    }
}

/// Borsh-encoded parameters for the `view` function.
#[derive(BorshSerialize, BorshDeserialize, Debug, Eq, PartialEq)]
pub struct ViewCallArgs {
    pub sender: RawAddress,
    pub address: RawAddress,
    pub amount: RawU256,
    pub input: Vec<u8>,
}

/// Borsh-encoded parameters for `deploy_erc20_token` function.
#[derive(BorshSerialize, BorshDeserialize, Debug, Eq, PartialEq, Clone)]
pub struct DeployErc20TokenArgs {
    pub nep141: AccountId,
}

/// Borsh-encoded parameters for `get_erc20_from_nep141` function.
pub type GetErc20FromNep141CallArgs = DeployErc20TokenArgs;

/// Borsh-encoded parameters for the `get_storage_at` function.
#[derive(BorshSerialize, BorshDeserialize)]
pub struct GetStorageAtArgs {
    pub address: RawAddress,
    pub key: RawH256,
}

/// Borsh-encoded (genesis) account balance used by the `begin_chain` function.
#[cfg(feature = "evm_bully")]
#[derive(BorshSerialize, BorshDeserialize)]
pub struct AccountBalance {
    pub address: RawAddress,
    pub balance: RawU256,
}

/// Borsh-encoded parameters for the `begin_chain` function.
#[cfg(feature = "evm_bully")]
#[derive(BorshSerialize, BorshDeserialize)]
pub struct BeginChainArgs {
    pub chain_id: RawU256,
    pub genesis_alloc: Vec<AccountBalance>,
}

/// Borsh-encoded parameters for the `begin_block` function.
#[cfg(feature = "evm_bully")]
#[derive(BorshSerialize, BorshDeserialize)]
pub struct BeginBlockArgs {
    /// The current block's hash (for replayer use).
    pub hash: RawU256,
    /// The current block's beneficiary address.
    pub coinbase: RawAddress,
    /// The current block's timestamp (in seconds since the Unix epoch).
    pub timestamp: RawU256,
    /// The current block's number (the genesis block is number zero).
    pub number: RawU256,
    /// The current block's difficulty.
    pub difficulty: RawU256,
    /// The current block's gas limit.
    pub gaslimit: RawU256,
}

/// Borsh-encoded parameters for the `ft_transfer_call` function
/// for regular NEP-141 tokens.
#[derive(Debug, Clone)]
pub struct NEP141FtOnTransferArgs {
    pub sender_id: AccountId,
    pub amount: Balance,
    pub msg: String,
}

impl TryFrom<JsonValue> for NEP141FtOnTransferArgs {
    type Error = JsonError;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        Ok(Self {
            sender_id: AccountId::try_from(value.string("sender_id")?)
                .map_err(|_| JsonError::InvalidString)?,
            amount: value.u128("amount")?,
            msg: value.string("msg")?,
        })
    }
}

impl From<NEP141FtOnTransferArgs> for String {
    fn from(value: NEP141FtOnTransferArgs) -> Self {
        format!(
            r#"{{"sender_id": "{}", "amount": "{}", "msg": "{}"}}"#,
            value.sender_id,
            value.amount,
            // Escape message to avoid json injection attacks
            value.msg.replace("\\", "\\\\").replace("\"", "\\\"")
        )
    }
}

/// Eth-connector deposit arguments
#[derive(BorshSerialize, BorshDeserialize)]
pub struct DepositCallArgs {
    /// Proof data
    pub proof: Proof,
    /// Optional relayer address
    pub relayer_eth_account: Option<EthAddress>,
}

/// Eth-connector isUsedProof arguments
#[derive(BorshSerialize, BorshDeserialize)]
pub struct IsUsedProofCallArgs {
    /// Proof data
    pub proof: Proof,
}

/// withdraw result for eth-connector
#[derive(BorshSerialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(BorshDeserialize))]
pub struct WithdrawResult {
    pub amount: Balance,
    pub recipient_id: RawAddress,
    pub eth_custodian_address: RawAddress,
}

/// Fungible token storage balance
#[derive(Default)]
pub struct StorageBalance {
    pub total: Balance,
    pub available: Balance,
}

impl StorageBalance {
    pub fn to_json_bytes(&self) -> Vec<u8> {
        format!(
            "{{\"total\": \"{}\", \"available\": \"{}\"}}",
            self.total.to_string(),
            self.available.to_string()
        )
        .as_bytes()
        .to_vec()
    }
}

/// ft_resolve_transfer eth-connector call args
#[derive(BorshSerialize, BorshDeserialize)]
pub struct ResolveTransferCallArgs {
    pub sender_id: AccountId,
    pub amount: Balance,
    pub receiver_id: AccountId,
}

/// Finish deposit NEAR eth-connector call args
#[derive(BorshSerialize, BorshDeserialize)]
pub struct FinishDepositCallArgs {
    pub new_owner_id: AccountId,
    pub amount: Balance,
    pub proof_key: String,
    pub relayer_id: AccountId,
    pub fee: Fee,
    pub msg: Option<Vec<u8>>,
}

/// Deposit ETH args
#[derive(Default, BorshDeserialize, BorshSerialize, Clone)]
pub struct DepositEthCallArgs {
    pub proof: Proof,
    pub relayer_eth_account: EthAddress,
}

/// Finish deposit NEAR eth-connector call args
#[derive(BorshSerialize, BorshDeserialize)]
pub struct FinishDepositEthCallArgs {
    pub new_owner_id: EthAddress,
    pub amount: Balance,
    pub fee: Balance,
    pub relayer_eth_account: AccountId,
    pub proof: Proof,
}

/// Eth-connector initial args
#[derive(BorshSerialize, BorshDeserialize)]
pub struct InitCallArgs {
    pub prover_account: AccountId,
    pub eth_custodian_address: String,
    pub metadata: FungibleTokenMetadata,
}

/// Eth-connector Set contract data call args
pub type SetContractDataCallArgs = InitCallArgs;

/// transfer eth-connector call args
#[derive(BorshSerialize, BorshDeserialize)]
pub struct TransferCallCallArgs {
    pub receiver_id: AccountId,
    pub amount: Balance,
    pub memo: Option<String>,
    pub msg: String,
}

impl TryFrom<JsonValue> for TransferCallCallArgs {
    type Error = error::ParseTypeFromJsonError;

    fn try_from(v: JsonValue) -> Result<Self, Self::Error> {
        let receiver_id = AccountId::try_from(v.string("receiver_id")?)?;
        let amount = v.u128("amount")?;
        let memo = v.string("memo").ok();
        let msg = v.string("msg")?;
        Ok(Self {
            receiver_id,
            amount,
            memo,
            msg,
        })
    }
}

/// storage_balance_of eth-connector call args
#[derive(BorshSerialize, BorshDeserialize)]
pub struct StorageBalanceOfCallArgs {
    pub account_id: crate::prelude::account_id::AccountId,
}

impl TryFrom<JsonValue> for StorageBalanceOfCallArgs {
    type Error = error::ParseTypeFromJsonError;

    fn try_from(v: JsonValue) -> Result<Self, Self::Error> {
        let account_id = AccountId::try_from(v.string("account_id")?)?;
        Ok(Self { account_id })
    }
}

/// storage_deposit eth-connector call args
#[derive(BorshSerialize, BorshDeserialize)]
pub struct StorageDepositCallArgs {
    pub account_id: Option<AccountId>,
    pub registration_only: Option<bool>,
}

impl From<JsonValue> for StorageDepositCallArgs {
    fn from(v: JsonValue) -> Self {
        Self {
            account_id: v
                .string("account_id")
                .map_or(None, |acc| AccountId::try_from(acc).ok()),
            registration_only: v.bool("registration_only").ok(),
        }
    }
}

/// storage_withdraw eth-connector call args
#[derive(BorshSerialize, BorshDeserialize)]
pub struct StorageWithdrawCallArgs {
    pub amount: Option<u128>,
}

impl From<JsonValue> for StorageWithdrawCallArgs {
    fn from(v: JsonValue) -> Self {
        Self {
            amount: v.u128("amount").ok(),
        }
    }
}

/// transfer args for json invocation
#[derive(BorshSerialize, BorshDeserialize)]
pub struct TransferCallArgs {
    pub receiver_id: AccountId,
    pub amount: Balance,
    pub memo: Option<String>,
}

impl TryFrom<JsonValue> for TransferCallArgs {
    type Error = error::ParseTypeFromJsonError;

    fn try_from(v: JsonValue) -> Result<Self, Self::Error> {
        Ok(Self {
            receiver_id: AccountId::try_from(v.string("receiver_id")?)?,
            amount: v.u128("amount")?,
            memo: v.string("memo").ok(),
        })
    }
}

/// balance_of args for json invocation
#[derive(BorshSerialize, BorshDeserialize)]
pub struct BalanceOfCallArgs {
    pub account_id: AccountId,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct BalanceOfEthCallArgs {
    pub address: EthAddress,
}

impl TryFrom<JsonValue> for BalanceOfCallArgs {
    type Error = error::ParseTypeFromJsonError;

    fn try_from(v: JsonValue) -> Result<Self, Self::Error> {
        Ok(Self {
            account_id: AccountId::try_from(v.string("account_id")?)?,
        })
    }
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct RegisterRelayerCallArgs {
    pub address: EthAddress,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct PauseEthConnectorCallArgs {
    pub paused_mask: PausedMask,
}

impl TryFrom<JsonValue> for ResolveTransferCallArgs {
    type Error = error::ParseTypeFromJsonError;

    fn try_from(v: JsonValue) -> Result<Self, Self::Error> {
        Ok(Self {
            sender_id: AccountId::try_from(v.string("sender_id")?)?,
            receiver_id: AccountId::try_from(v.string("receiver_id")?)?,
            amount: v.u128("amount")?,
        })
    }
}

pub mod error {
    use crate::json::JsonError;
    use aurora_engine_types::account_id::ParseAccountError;

    pub enum ParseTypeFromJsonError {
        Json(JsonError),
        InvalidAccount(ParseAccountError),
    }

    impl From<JsonError> for ParseTypeFromJsonError {
        fn from(e: JsonError) -> Self {
            Self::Json(e)
        }
    }

    impl From<ParseAccountError> for ParseTypeFromJsonError {
        fn from(e: ParseAccountError) -> Self {
            Self::InvalidAccount(e)
        }
    }

    impl AsRef<[u8]> for ParseTypeFromJsonError {
        fn as_ref(&self) -> &[u8] {
            match self {
                Self::Json(e) => e.as_ref(),
                Self::InvalidAccount(e) => e.as_ref(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_call_fail() {
        let bytes = [0; 71];
        let _ = ViewCallArgs::try_from_slice(&bytes).unwrap_err();
    }

    #[test]
    fn test_roundtrip_view_call() {
        let x = ViewCallArgs {
            sender: [1; 20],
            address: [2; 20],
            amount: [3; 32],
            input: vec![1, 2, 3],
        };
        let bytes = x.try_to_vec().unwrap();
        let res = ViewCallArgs::try_from_slice(&bytes).unwrap();
        assert_eq!(x, res);
    }

    #[test]
    fn test_call_args_deserialize() {
        let new_input = FunctionCallArgsV2 {
            contract: [0u8; 20],
            value: WeiU256::default(),
            input: Vec::new(),
        };
        let legacy_input = FunctionCallArgsV1 {
            contract: [0u8; 20],
            input: Vec::new(),
        };

        // Parsing bytes in a new input format - data structures (wrapped into call args enum) with new arguments,
        // made for flexibility and extensibility.

        // Using new input format (wrapped into call args enum) and data structure with new argument (`value` field).
        let input_bytes = CallArgs::V2(new_input.clone()).try_to_vec().unwrap();
        let parsed_data = CallArgs::deserialize(&input_bytes);
        assert_eq!(parsed_data, Some(CallArgs::V2(new_input.clone())));

        // Using new input format (wrapped into call args enum) and old data structure with legacy arguments,
        // this is allowed for compatibility reason.
        let input_bytes = CallArgs::V1(legacy_input.clone()).try_to_vec().unwrap();
        let parsed_data = CallArgs::deserialize(&input_bytes);
        assert_eq!(parsed_data, Some(CallArgs::V1(legacy_input.clone())));

        // Parsing bytes in an old input format - raw data structure (not wrapped into call args enum) with legacy arguments,
        // made for backward compatibility.

        // Using old input format (not wrapped into call args enum) - raw data structure with legacy arguments.
        let input_bytes = legacy_input.try_to_vec().unwrap();
        let parsed_data = CallArgs::deserialize(&input_bytes);
        assert_eq!(parsed_data, Some(CallArgs::V1(legacy_input.clone())));

        // Using old input format (not wrapped into call args enum) - raw data structure with new argument (`value` field).
        // Data structures with new arguments allowed only in new input format for future extensibility reason.
        // Raw data structure (old input format) allowed only with legacy arguments for backward compatibility reason.
        // Unrecognized input should be handled and result as an exception in a call site.
        let input_bytes = new_input.try_to_vec().unwrap();
        let parsed_data = CallArgs::deserialize(&input_bytes);
        assert_eq!(parsed_data, None);
    }
}
