use super::{EvmPrecompileResult, Precompile};
#[cfg(feature = "contract")]
use crate::prelude::{
    format,
    parameters::{PromiseArgs, PromiseCreateArgs, WithdrawCallArgs},
    sdk,
    storage::{bytes_to_key, KeyPrefix},
    vec, BorshSerialize, Cow, String, ToString, TryFrom, TryInto, Vec, H160, U256,
};
#[cfg(all(feature = "error_refund", feature = "contract"))]
use crate::prelude::{
    parameters::{PromiseWithCallbackArgs, RefundCallArgs},
    types,
};

use crate::prelude::types::EthGas;
use crate::prelude::Address;
use crate::PrecompileOutput;
use aurora_engine_types::account_id::AccountId;
#[cfg(feature = "contract")]
use evm::backend::Log;
use evm::{Context, ExitError};

const ERR_TARGET_TOKEN_NOT_FOUND: &str = "Target token not found";

mod costs {
    use crate::prelude::types::EthGas;

    // TODO(#51): Determine the correct amount of gas
    pub(super) const EXIT_TO_NEAR_GAS: EthGas = EthGas::new(0);

    // TODO(#51): Determine the correct amount of gas
    pub(super) const EXIT_TO_ETHEREUM_GAS: EthGas = EthGas::new(0);

    // TODO(#332): Determine the correct amount of gas
    pub(super) const FT_TRANSFER_GAS: EthGas = EthGas::new(100_000_000_000_000);

    // TODO(#332): Determine the correct amount of gas
    #[cfg(feature = "error_refund")]
    pub(super) const REFUND_ON_ERROR_GAS: EthGas = EthGas::new(60_000_000_000_000);

    // TODO(#332): Determine the correct amount of gas
    pub(super) const WITHDRAWAL_GAS: EthGas = EthGas::new(100_000_000_000_000);
}

pub mod events {
    use crate::prelude::{vec, Address, String, ToString, H256, U256};

    /// Derived from event signature (see tests::test_exit_signatures)
    pub const EXIT_TO_NEAR_SIGNATURE: H256 = crate::make_h256(
        0x5a91b8bc9c1981673db8fb226dbd8fcd,
        0xd0c23f45cd28abb31403a5392f6dd0c7,
    );
    /// Derived from event signature (see tests::test_exit_signatures)
    pub const EXIT_TO_ETH_SIGNATURE: H256 = crate::make_h256(
        0xd046c2bb01a5622bc4b9696332391d87,
        0x491373762eeac0831c48400e2d5a5f07,
    );

    /// The exit precompile events have an `erc20_address` field to indicate
    /// which ERC-20 token is being withdrawn. However, ETH is not an ERC-20 token
    /// So we need to have some other address to fill this field. This constant is
    /// used for this purpose.
    pub const ETH_ADDRESS: Address = Address([0; 20]);

    /// ExitToNear(
    ///    Address indexed sender,
    ///    Address indexed erc20_address,
    ///    string indexed dest,
    ///    uint amount
    /// )
    /// Note: in the ERC-20 exit case `sender` == `erc20_address` because it is
    /// the ERC-20 contract which calls the exit precompile. However in the case
    /// of ETH exit the sender will give the true sender (and the `erc20_address`
    /// will not be meaningful because ETH is not an ERC-20 token).
    pub struct ExitToNear {
        pub sender: Address,
        pub erc20_address: Address,
        pub dest: String,
        pub amount: U256,
    }

    impl ExitToNear {
        pub fn encode(self) -> ethabi::RawLog {
            let data = ethabi::encode(&[ethabi::Token::Int(self.amount)]);
            let topics = vec![
                EXIT_TO_NEAR_SIGNATURE,
                encode_address(self.sender),
                encode_address(self.erc20_address),
                aurora_engine_sdk::keccak(&ethabi::encode(&[ethabi::Token::String(self.dest)])),
            ];

            ethabi::RawLog { topics, data }
        }
    }

    /// ExitToEth(
    ///    Address indexed sender,
    ///    Address indexed erc20_address,
    ///    string indexed dest,
    ///    uint amount
    /// )
    /// Note: in the ERC-20 exit case `sender` == `erc20_address` because it is
    /// the ERC-20 contract which calls the exit precompile. However in the case
    /// of ETH exit the sender will give the true sender (and the `erc20_address`
    /// will not be meaningful because ETH is not an ERC-20 token).
    pub struct ExitToEth {
        pub sender: Address,
        pub erc20_address: Address,
        pub dest: Address,
        pub amount: U256,
    }

    impl ExitToEth {
        pub fn encode(self) -> ethabi::RawLog {
            let data = ethabi::encode(&[ethabi::Token::Int(self.amount)]);
            let topics = vec![
                EXIT_TO_ETH_SIGNATURE,
                encode_address(self.sender),
                encode_address(self.erc20_address),
                encode_address(self.dest),
            ];

            ethabi::RawLog { topics, data }
        }
    }

    fn encode_address(a: Address) -> H256 {
        let mut result = [0u8; 32];
        result[12..].copy_from_slice(a.as_ref());
        H256(result)
    }

    pub fn exit_to_near_schema() -> ethabi::Event {
        ethabi::Event {
            name: "ExitToNear".to_string(),
            inputs: vec![
                ethabi::EventParam {
                    name: "sender".to_string(),
                    kind: ethabi::ParamType::Address,
                    indexed: true,
                },
                ethabi::EventParam {
                    name: "erc20_address".to_string(),
                    kind: ethabi::ParamType::Address,
                    indexed: true,
                },
                ethabi::EventParam {
                    name: "dest".to_string(),
                    kind: ethabi::ParamType::String,
                    indexed: true,
                },
                ethabi::EventParam {
                    name: "amount".to_string(),
                    kind: ethabi::ParamType::Uint(256),
                    indexed: false,
                },
            ],
            anonymous: false,
        }
    }

    pub fn exit_to_eth_schema() -> ethabi::Event {
        ethabi::Event {
            name: "ExitToEth".to_string(),
            inputs: vec![
                ethabi::EventParam {
                    name: "sender".to_string(),
                    kind: ethabi::ParamType::Address,
                    indexed: true,
                },
                ethabi::EventParam {
                    name: "erc20_address".to_string(),
                    kind: ethabi::ParamType::Address,
                    indexed: true,
                },
                ethabi::EventParam {
                    name: "dest".to_string(),
                    kind: ethabi::ParamType::Address,
                    indexed: true,
                },
                ethabi::EventParam {
                    name: "amount".to_string(),
                    kind: ethabi::ParamType::Uint(256),
                    indexed: false,
                },
            ],
            anonymous: false,
        }
    }
}

//TransferEthToNear
pub struct ExitToNear {
    current_account_id: AccountId,
}

impl ExitToNear {
    /// Exit to NEAR precompile address
    ///
    /// Address: `0xe9217bc70b7ed1f598ddd3199e80b093fa71124f`
    /// This address is computed as: `&keccak("exitToNear")[12..]`
    pub const ADDRESS: Address =
        super::make_address(0xe9217bc7, 0x0b7ed1f598ddd3199e80b093fa71124f);

    pub fn new(current_account_id: AccountId) -> Self {
        Self { current_account_id }
    }
}

#[cfg(feature = "contract")]
fn get_nep141_from_erc20(erc20_token: &[u8]) -> AccountId {
    use sdk::io::{StorageIntermediate, IO};
    AccountId::try_from(
        sdk::near_runtime::Runtime
            .read_storage(bytes_to_key(KeyPrefix::Erc20Nep141Map, erc20_token).as_slice())
            .map(|s| s.to_vec())
            .expect(ERR_TARGET_TOKEN_NOT_FOUND),
    )
    .unwrap()
}

impl Precompile for ExitToNear {
    fn required_gas(_input: &[u8]) -> Result<EthGas, ExitError> {
        Ok(costs::EXIT_TO_NEAR_GAS)
    }

    #[cfg(not(feature = "contract"))]
    fn run(
        &self,
        input: &[u8],
        target_gas: Option<EthGas>,
        _context: &Context,
        _is_static: bool,
    ) -> EvmPrecompileResult {
        if let Some(target_gas) = target_gas {
            if Self::required_gas(input)? > target_gas {
                return Err(ExitError::OutOfGas);
            }
        }

        Ok(PrecompileOutput::default().into())
    }

    #[cfg(feature = "contract")]
    fn run(
        &self,
        input: &[u8],
        target_gas: Option<EthGas>,
        context: &Context,
        is_static: bool,
    ) -> EvmPrecompileResult {
        #[cfg(feature = "error_refund")]
        fn parse_input(input: &[u8]) -> (Address, &[u8]) {
            let refund_address = Address::from_slice(&input[1..21]);
            (refund_address, &input[21..])
        }
        #[cfg(not(feature = "error_refund"))]
        fn parse_input(input: &[u8]) -> &[u8] {
            &input[1..]
        }

        if let Some(target_gas) = target_gas {
            if Self::required_gas(input)? > target_gas {
                return Err(ExitError::OutOfGas);
            }
        }

        // It's not allowed to call exit precompiles in static mode
        if is_static {
            return Err(ExitError::Other(Cow::from("ERR_INVALID_IN_STATIC")));
        }

        // First byte of the input is a flag, selecting the behavior to be triggered:
        //      0x0 -> Eth transfer
        //      0x1 -> Erc20 transfer
        let flag = input[0];
        #[cfg(feature = "error_refund")]
        let (refund_address, mut input) = parse_input(input);
        #[cfg(not(feature = "error_refund"))]
        let mut input = parse_input(input);
        let current_account_id = self.current_account_id.clone();
        #[cfg(feature = "error_refund")]
        let refund_on_error_target = current_account_id.clone();

        let (nep141_address, args, exit_event) = match flag {
            0x0 => {
                // ETH transfer
                //
                // Input slice format:
                //      recipient_account_id (bytes) - the NEAR recipient account which will receive NEP-141 ETH tokens

                if let Ok(dest_account) = AccountId::try_from(input) {
                    (
                        current_account_id,
                        // There is no way to inject json, given the encoding of both arguments
                        // as decimal and valid account id respectively.
                        format!(
                            r#"{{"receiver_id": "{}", "amount": "{}", "memo": null}}"#,
                            dest_account,
                            context.apparent_value.as_u128()
                        ),
                        events::ExitToNear {
                            sender: context.caller,
                            erc20_address: events::ETH_ADDRESS,
                            dest: dest_account.to_string(),
                            amount: context.apparent_value,
                        },
                    )
                } else {
                    return Err(ExitError::Other(Cow::from(
                        "ERR_INVALID_RECEIVER_ACCOUNT_ID",
                    )));
                }
            }
            0x1 => {
                // ERC20 transfer
                //
                // This precompile branch is expected to be called from the ERC20 burn function\
                //
                // Input slice format:
                //      amount (U256 big-endian bytes) - the amount that was burned
                //      recipient_account_id (bytes) - the NEAR recipient account which will receive NEP-141 tokens

                if context.apparent_value != U256::from(0) {
                    return Err(ExitError::Other(Cow::from(
                        "ERR_ETH_ATTACHED_FOR_ERC20_EXIT",
                    )));
                }

                let erc20_address = context.caller;
                let nep141_address = get_nep141_from_erc20(erc20_address.as_bytes());

                let amount = U256::from_big_endian(&input[..32]);
                input = &input[32..];

                if let Ok(receiver_account_id) = AccountId::try_from(input) {
                    (
                        nep141_address,
                        // There is no way to inject json, given the encoding of both arguments
                        // as decimal and valid account id respectively.
                        format!(
                            r#"{{"receiver_id": "{}", "amount": "{}", "memo": null}}"#,
                            receiver_account_id,
                            amount.as_u128()
                        ),
                        events::ExitToNear {
                            sender: erc20_address,
                            erc20_address,
                            dest: receiver_account_id.to_string(),
                            amount,
                        },
                    )
                } else {
                    return Err(ExitError::Other(Cow::from(
                        "ERR_INVALID_RECEIVER_ACCOUNT_ID",
                    )));
                }
            }
            _ => return Err(ExitError::Other(Cow::from("ERR_INVALID_FLAG"))),
        };

        #[cfg(feature = "error_refund")]
        let erc20_address = if flag == 0 {
            None
        } else {
            Some(exit_event.erc20_address.0)
        };
        #[cfg(feature = "error_refund")]
        let refund_args = RefundCallArgs {
            recipient_address: refund_address.0,
            erc20_address,
            amount: types::u256_to_arr(&exit_event.amount),
        };
        #[cfg(feature = "error_refund")]
        let refund_promise = PromiseCreateArgs {
            target_account_id: refund_on_error_target,
            method: "refund_on_error".to_string(),
            args: refund_args.try_to_vec().unwrap(),
            attached_balance: 0,
            attached_gas: costs::REFUND_ON_ERROR_GAS.into_u64(),
        };
        let transfer_promise = PromiseCreateArgs {
            target_account_id: nep141_address,
            method: "ft_transfer".to_string(),
            args: args.as_bytes().to_vec(),
            attached_balance: 1,
            attached_gas: costs::FT_TRANSFER_GAS.into_u64(),
        };

        #[cfg(feature = "error_refund")]
        let promise = PromiseArgs::Callback(PromiseWithCallbackArgs {
            base: transfer_promise,
            callback: refund_promise,
        });
        #[cfg(not(feature = "error_refund"))]
        let promise = PromiseArgs::Create(transfer_promise);

        let promise_log = Log {
            address: Self::ADDRESS,
            topics: Vec::new(),
            data: promise.try_to_vec().unwrap(),
        };
        let exit_event_log = exit_event.encode();
        let exit_event_log = Log {
            address: Self::ADDRESS,
            topics: exit_event_log.topics,
            data: exit_event_log.data,
        };

        Ok(PrecompileOutput {
            logs: vec![promise_log, exit_event_log],
            ..Default::default()
        }
        .into())
    }
}

pub struct ExitToEthereum {
    current_account_id: AccountId,
}

impl ExitToEthereum {
    /// Exit to Ethereum precompile address
    ///
    /// Address: `0xb0bd02f6a392af548bdf1cfaee5dfa0eefcc8eab`
    /// This address is computed as: `&keccak("exitToEthereum")[12..]`
    pub const ADDRESS: Address =
        super::make_address(0xb0bd02f6, 0xa392af548bdf1cfaee5dfa0eefcc8eab);

    pub fn new(current_account_id: AccountId) -> Self {
        Self { current_account_id }
    }
}

impl Precompile for ExitToEthereum {
    fn required_gas(_input: &[u8]) -> Result<EthGas, ExitError> {
        Ok(costs::EXIT_TO_ETHEREUM_GAS)
    }

    #[cfg(not(feature = "contract"))]
    fn run(
        &self,
        input: &[u8],
        target_gas: Option<EthGas>,
        _context: &Context,
        _is_static: bool,
    ) -> EvmPrecompileResult {
        if let Some(target_gas) = target_gas {
            if Self::required_gas(input)? > target_gas {
                return Err(ExitError::OutOfGas);
            }
        }

        Ok(PrecompileOutput::default().into())
    }

    #[cfg(feature = "contract")]
    fn run(
        &self,
        input: &[u8],
        target_gas: Option<EthGas>,
        context: &Context,
        is_static: bool,
    ) -> EvmPrecompileResult {
        if let Some(target_gas) = target_gas {
            if Self::required_gas(input)? > target_gas {
                return Err(ExitError::OutOfGas);
            }
        }

        // It's not allowed to call exit precompiles in static mode
        if is_static {
            return Err(ExitError::Other(Cow::from("ERR_INVALID_IN_STATIC")));
        }

        // First byte of the input is a flag, selecting the behavior to be triggered:
        //      0x0 -> Eth transfer
        //      0x1 -> Erc20 transfer
        let mut input = input;
        let flag = input[0];
        input = &input[1..];

        let (nep141_address, serialized_args, exit_event) = match flag {
            0x0 => {
                // ETH transfer
                //
                // Input slice format:
                //      eth_recipient (20 bytes) - the address of recipient which will receive ETH on Ethereum
                let recipient_address = input
                    .try_into()
                    .map_err(|_| ExitError::Other(Cow::from("ERR_INVALID_RECIPIENT_ADDRESS")))?;
                (
                    self.current_account_id.clone(),
                    // There is no way to inject json, given the encoding of both arguments
                    // as decimal and hexadecimal respectively.
                    WithdrawCallArgs {
                        recipient_address,
                        amount: context.apparent_value.as_u128(),
                    }
                    .try_to_vec()
                    .map_err(|_| ExitError::Other(Cow::from("ERR_INVALID_AMOUNT")))?,
                    events::ExitToEth {
                        sender: context.caller,
                        erc20_address: events::ETH_ADDRESS,
                        dest: H160(recipient_address),
                        amount: context.apparent_value,
                    },
                )
            }
            0x1 => {
                // ERC-20 transfer
                //
                // This precompile branch is expected to be called from the ERC20 withdraw function
                // (or burn function with some flag provided that this is expected to be withdrawn)
                //
                // Input slice format:
                //      amount (U256 big-endian bytes) - the amount that was burned
                //      eth_recipient (20 bytes) - the address of recipient which will receive ETH on Ethereum

                if context.apparent_value != U256::from(0) {
                    return Err(ExitError::Other(Cow::from(
                        "ERR_ETH_ATTACHED_FOR_ERC20_EXIT",
                    )));
                }

                let erc20_address = context.caller;
                let nep141_address = get_nep141_from_erc20(erc20_address.as_bytes());

                let amount = U256::from_big_endian(&input[..32]);
                input = &input[32..];

                if input.len() == 20 {
                    // Parse ethereum address in hex
                    let eth_recipient: String = hex::encode(input.to_vec());
                    // unwrap cannot fail since we checked the length already
                    let recipient_address = input.try_into().unwrap();

                    (
                        nep141_address,
                        // There is no way to inject json, given the encoding of both arguments
                        // as decimal and hexadecimal respectively.
                        format!(
                            r#"{{"amount": "{}", "recipient": "{}"}}"#,
                            amount.as_u128(),
                            eth_recipient
                        )
                        .as_bytes()
                        .to_vec(),
                        events::ExitToEth {
                            sender: erc20_address,
                            erc20_address,
                            dest: H160(recipient_address),
                            amount,
                        },
                    )
                } else {
                    return Err(ExitError::Other(Cow::from("ERR_INVALID_RECIPIENT_ADDRESS")));
                }
            }
            _ => {
                return Err(ExitError::Other(Cow::from(
                    "ERR_INVALID_RECEIVER_ACCOUNT_ID",
                )));
            }
        };

        let withdraw_promise = PromiseCreateArgs {
            target_account_id: nep141_address,
            method: "withdraw".to_string(),
            args: serialized_args,
            attached_balance: 1,
            attached_gas: costs::WITHDRAWAL_GAS.into_u64(),
        };

        let promise = PromiseArgs::Create(withdraw_promise).try_to_vec().unwrap();
        let promise_log = Log {
            address: Self::ADDRESS,
            topics: Vec::new(),
            data: promise,
        };
        let exit_event_log = exit_event.encode();
        let exit_event_log = Log {
            address: Self::ADDRESS,
            topics: exit_event_log.topics,
            data: exit_event_log.data,
        };

        Ok(PrecompileOutput {
            logs: vec![promise_log, exit_event_log],
            ..Default::default()
        }
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::{ExitToEthereum, ExitToNear};
    use crate::prelude::sdk::types::near_account_to_evm_address;

    #[test]
    fn test_precompile_id() {
        assert_eq!(
            ExitToEthereum::ADDRESS,
            near_account_to_evm_address("exitToEthereum".as_bytes())
        );
        assert_eq!(
            ExitToNear::ADDRESS,
            near_account_to_evm_address("exitToNear".as_bytes())
        );
    }

    #[test]
    fn test_exit_signatures() {
        let exit_to_near = super::events::exit_to_near_schema();
        let exit_to_eth = super::events::exit_to_eth_schema();

        assert_eq!(
            exit_to_near.signature(),
            super::events::EXIT_TO_NEAR_SIGNATURE
        );
        assert_eq!(
            exit_to_eth.signature(),
            super::events::EXIT_TO_ETH_SIGNATURE
        );
    }
}
