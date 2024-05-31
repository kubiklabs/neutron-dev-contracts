use astroport::asset::validate_native_denom;
use astroport::tokenfactory_tracker::{InstantiateMsg, SudoMsg};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError, Storage, Uint128};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::state::{Config, BALANCES, CONFIG, TOTAL_SUPPLY_HISTORY};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    deps.api.addr_validate(&msg.tokenfactory_module_address)?;

    validate_native_denom(&msg.tracked_denom)?;

    let config = Config {
        d: msg.tracked_denom.clone(),
        m: msg.tokenfactory_module_address,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default()
        .add_attribute("action", "instantiate")
        .add_attribute("contract", CONTRACT_NAME)
        .add_attribute("tracked_denom", config.d)
        .add_attribute("tokenfactory_module_address", config.m))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        // BlockBeforeSend is called before a send - if an error is returned the send is cancelled.
        // This call doesn't have gas limitations but the gas used due to calling this contract contributes to the total tx gas.
        // Extended bank module calls BlockBeforeSend and TrackBeforeSend sequentially on mint, send and burn actions.
        // Ref: https://github.com/neutron-org/cosmos-sdk/blob/28f3db48a7ae038e9ccdd2bae632cb21c1c9de86/x/bank/keeper/send.go#L207-L223
        SudoMsg::BlockBeforeSend { from, to, amount } => {
            let config = CONFIG.load(deps.storage)?;

            // Ensure the denom being sent is the tracked denom
            // If this isn't checked, another token could be tracked with the same
            // contract and that will skew the real numbers
            if amount.denom != config.d {
                Err(ContractError::InvalidDenom {
                    expected_denom: config.d,
                })
            } else {
                // If this function throws error all send, mint and burn actions will be blocked.
                // However, balances query will still work, hence governance will be able to recover the contract.
                track_balances(
                    deps.storage,
                    env.block.time.seconds(),
                    &config,
                    from,
                    to,
                    amount.amount,
                )
            }
        }
        // tokenfactory enforces hard gas limit 100k on TrackBeforeSend of which 60k is a flat contract initialization.
        // Hence, we have only up to 40k gas to handle our logic. If TrackBeforeSend hits the limit it is silently ignored on chain level,
        // making balance tracking broken with no way to recover.
        // Balance tracking feature is crucial for Astroport and Neutron DAOs thus we deliberately abuse SudoMsg::BlockBeforeSend
        // because it is not gas metered and we can do all the logic we need.
        // Ref: https://github.com/neutron-org/neutron/blob/57a25eb719eb0db973543f9d54ace484ac098721/x/tokenfactory/keeper/before_send.go#L143-L150
        SudoMsg::TrackBeforeSend { .. } => Ok(Response::default()),
    }
}

/// Track balance and total supply changes over timestamp.
/// Only tokenfactory module itself can change supply by minting and burning tokens.
/// Only denom admin can dispatch mint/burn messages to the module.
/// Sending tokens to the tokenfactory module address isn't allowed by the chain.
/// Thus,
/// - if from == module_address -> mint
/// - if to == module_address -> burn
/// - other scenarios are simple transfers between addresses
/// Possible errors:
/// - serialization/deserialization errors. Should never happen if both BALANCES and TOTAL_SUPPLY_HISTORY storage keys and data layout are not changed.
/// - attempt to subtract from zero balance or reduce empty total supply. Highly unlikely possible. Might happen due to errors in the tokenfactory module.
/// - attempt to add with overflow. First will happen on total supply increase. Possible if total supply is greater than 2^128 - 1.
pub fn track_balances(
    storage: &mut dyn Storage,
    block_seconds: u64,
    config: &Config,
    from: String,
    to: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    // If the token is minted directly to an address, we don't need to subtract
    // as the sender is the module address
    if from.ne(&config.m) {
        BALANCES.update::<_, StdError>(storage, &from, block_seconds, |balance| {
            balance
                .unwrap_or_default()
                .checked_sub(amount)
                .map_err(|err| {
                    StdError::generic_err(format!(
                        "{err}: send from {from} to {to} amount {amount} block_seconds {block_seconds}"
                    ))
                })
        })?;
    } else {
        // Minted new tokens
        TOTAL_SUPPLY_HISTORY.update::<_, StdError>(storage, block_seconds, |balance| {
            Ok(balance.unwrap_or_default().checked_add(amount)?)
        })?;
    }

    // When burning tokens, the receiver is the token factory module address
    // Sending tokens to the module address isn't allowed by the chain
    if to.ne(&config.m) {
        BALANCES.update::<_, StdError>(storage, &to, block_seconds, |balance| {
            Ok(balance.unwrap_or_default().checked_add(amount)?)
        })?;
    } else {
        // Burned tokens
        TOTAL_SUPPLY_HISTORY.update::<_, StdError>(storage, block_seconds, |balance| {
            balance
                .unwrap_or_default()
                .checked_sub(amount)
                .map_err(|err| {
                    StdError::generic_err(format!(
                        "{err}: from {from} to {to} amount {amount} block_seconds {block_seconds}"
                    ))
                })
        })?;
    }

    Ok(Response::default())
}
