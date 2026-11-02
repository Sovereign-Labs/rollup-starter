// To parse this data:
//
//   import { Convert, RuntimeCall } from "./file";
//
//   const runtimeCall = Convert.toRuntimeCall(json);
//
// These functions will throw an error if the JSON doesn't
// match the expected interface, even if the JSON is valid.

/**
 * This enum is generated from the underlying Runtime, the variants correspond to call
 * messages from the relevant modules
 *
 * Module call message.
 */
export interface RuntimeCall {
    accounts?:                 Accounts;
    uniqueness?:               null;
    bank?:                     Bank;
    sequencer_registry?:       SequencerRegistry;
    operator_incentives?:      OperatorIncentives;
    attester_incentives?:      AttesterIncentivesClass | AttesterIncentivesEnum;
    prover_incentives?:        ProverIncentivesClass | ProverIncentivesEnum;
    chain_state?:              ChainStateClass | ChainStateEnum;
    blob_storage?:             null;
    paymaster?:                Paymaster;
    revenue_share?:            RevenueShareClass | RevenueShareEnum;
    mailbox?:                  Mailbox;
    interchain_gas_paymaster?: InterchainGasPaymaster;
    merkle_tree_hook?:         null;
    warp?:                     Warp;
    value_setter?:             ValueSetter;
    evm?:                      Evm;
}

/**
 * Represents the available call messages for interacting with the sov-accounts module.
 *
 * Authorizes `credential_id` as a signer for the caller's address.
 * Fails if the credential is already authorized for the caller's address.
 *
 * Authorizes `credential` to sign transactions that execute as `address`.
 * The caller must currently be signing as `address`. Fails if the tuple
 * is already authorized.
 *
 * Revokes `credential` from `address`. The caller must currently be
 * signing as `address`. No orphan guard: revoking the last credential
 * succeeds and leaves `address` unspendable via this map.
 *
 * Atomically swaps `old_credential` for `new_credential` on `address`.
 * Functionally equivalent to a `RemoveCredentialFromAddress` followed by
 * an `AddCredentialToAddress`, collapsed into a single call so the
 * caller does not have to authorize two transactions during a rotation.
 * The caller must currently be signing as `address`.
 *
 * Creates a new *synthetic* address — an address whose authorization
 * lives purely in `account_owners` and which has no
 * naturally-corresponding private key — and auto-authorizes the
 * caller's current credential for it.
 *
 * The address is derived deterministically by hashing
 * `(domain || visible_slot_hash || sender_addr || sender_credential || salt)`
 * with `S::CryptoSpec::Hasher`, then converting the resulting 32 bytes
 * to `S::Address` via `CredentialId.into()`. Different callers, salts,
 * and visible slots produce different addresses; replaying the same
 * tuple in the same slot is an idempotent no-op.
 *
 * `visible_slot_hash` is part of the derivation by design: an
 * attacker who later compromises the caller's private key cannot
 * reconstruct the same synthetic address off-chain, because the slot
 * hash only becomes known once chain progress commits to it and is
 * unforgeable without participating in consensus. This makes the
 * call *bind-then-use*, not *counterfactual*: unlike CREATE2 or
 * ERC-4337, the address cannot be predicted and prefunded ahead of
 * the `CreateSyntheticAddress` transaction. Callers must wait for
 * finalization and read the derived address from the
 * `SyntheticAddressCreated` event before routing assets or
 * permissions to it.
 */
export interface Accounts {
    /**
     * The credential id being authorized.
     */
    insert_credential_id?:           string;
    add_credential_to_address?:      AddCredentialToAddress;
    remove_credential_from_address?: RemoveCredentialFromAddress;
    rotate_credential_on_address?:   RotateCredentialOnAddress;
    create_synthetic_address?:       CreateSyntheticAddress;
}

export interface AddCredentialToAddress {
    /**
     * The address whose credential set is being extended. Must equal
     * `context.sender()`.
     */
    address: string;
    /**
     * The credential being authorized for `address`.
     */
    credential: string;
    [property: string]: any;
}

export interface CreateSyntheticAddress {
    /**
     * Caller-supplied salt that allows the same caller to derive
     * multiple distinct synthetic addresses in the same slot.
     */
    salt: number[];
    [property: string]: any;
}

export interface RemoveCredentialFromAddress {
    /**
     * The address whose credential set is being reduced. Must equal
     * `context.sender()`.
     */
    address: string;
    /**
     * The credential being revoked from `address`.
     */
    credential: string;
    [property: string]: any;
}

export interface RotateCredentialOnAddress {
    /**
     * The address whose credential set is being rotated. Must equal
     * `context.sender()`.
     */
    address: string;
    /**
     * The credential being authorized for `address`. Must not already
     * be authorized.
     */
    new_credential: string;
    /**
     * The credential being revoked from `address`. Must be currently
     * authorized.
     */
    old_credential: string;
    [property: string]: any;
}

/**
 * Register an attester, the parameter is the bond amount
 *
 * Register a challenger, the parameter is the bond amount
 *
 * Increases the balance of the attester.
 */
export interface AttesterIncentivesClass {
    register_attester?:   string;
    register_challenger?: string;
    deposit_attester?:    string;
}

/**
 * Start the first phase of the two-phase exit process
 *
 * Finish the two phase exit
 *
 * Exit a challenger
 */
export enum AttesterIncentivesEnum {
    BeginExitAttester = "begin_exit_attester",
    ExitAttester = "exit_attester",
    ExitChallenger = "exit_challenger",
}

/**
 * This enumeration represents the available call messages for interacting with the sov-bank
 * module.
 *
 * Creates a new token with the specified name and initial balance.
 *
 * Transfers a specified amount of tokens to the specified address.
 *
 * Burns a specified amount of tokens.
 *
 * Mints a specified amount of tokens.
 *
 * Freezes a token so that the supply is frozen
 *
 * Updates the list of admins for a specified token.
 */
export interface Bank {
    create_token?:       CreateToken;
    transfer?:           Transfer;
    burn?:               Burn;
    mint?:               Mint;
    freeze?:             Freeze;
    update_admin?:       UpdateAdmin;
    transfer_with_memo?: TransferWithMemo;
}

export interface Burn {
    /**
     * The amount of tokens to burn.
     */
    coins: Coins;
    [property: string]: any;
}

/**
 * The amount of tokens to transfer.
 *
 * Structure that stores information specifying
 * a given `amount` (type [`Amount`]) of coins stored at a `token_id`
 * (type [`crate::TokenId`]).
 *
 * The amount of tokens to burn.
 *
 * The amount of tokens to mint.
 */
export interface Coins {
    /**
     * The number of tokens
     */
    amount: string;
    /**
     * The ID of the token
     */
    token_id: string;
    [property: string]: any;
}

export interface CreateToken {
    /**
     * Admins list.
     */
    admins: string[];
    /**
     * The initial balance of the new token.
     */
    initial_balance: string;
    /**
     * The address of the account that the new tokens are minted to.
     */
    mint_to_address: string;
    /**
     * The supply cap of the new token, if any.
     */
    supply_cap?: null | string;
    /**
     * The number of decimal places this token's amounts will have.
     */
    token_decimals?: number | null;
    /**
     * The name of the new token.
     */
    token_name: string;
    [property: string]: any;
}

export interface Freeze {
    /**
     * Address of the token to be frozen
     */
    token_id: string;
    [property: string]: any;
}

export interface Mint {
    /**
     * The amount of tokens to mint.
     */
    coins: Coins;
    /**
     * Address to mint tokens to
     */
    mint_to_address: string;
    [property: string]: any;
}

export interface Transfer {
    /**
     * The amount of tokens to transfer.
     */
    coins: Coins;
    /**
     * The address to which the tokens will be transferred.
     */
    to: string;
    [property: string]: any;
}

export interface TransferWithMemo {
    /**
     * The amount of tokens to transfer.
     */
    coins: Coins;
    /**
     * The message included with the transfer
     */
    memo: string;
    /**
     * The address to which the tokens will be transferred.
     */
    to: string;
    [property: string]: any;
}

export interface UpdateAdmin {
    /**
     * The new admin address.
     * If `None`, the current admin entry for the transaction sender will be removed.
     */
    new_admin?: null | string;
    /**
     * The ID of the token whose admin list is being updated.
     */
    token_id: string;
    [property: string]: any;
}

/**
 * Sets the current time.
 */
export interface ChainStateClass {
    SetOracleTime: SetOracleTime;
}

export interface SetOracleTime {
    /**
     * The new time in milliseconds since the epoch
     */
    milliseconds_since_epoch: number;
    [property: string]: any;
}

/**
 * Terminates setup mode as of the next rollup block.
 */
export enum ChainStateEnum {
    TerminateSetupMode = "TerminateSetupMode",
}

/**
 * EVM call message.
 *
 * RLP encoded transaction.
 *
 * Update the runtime configuration
 *
 * Enable or disable custom EVM precompiles.
 *
 * This is a dedicated message (rather than a field of [`EvmRuntimeConfigUpdate`]) so that
 * the
 * borsh layout of the pre-existing call messages is preserved: appending a variant keeps
 * the
 * existing variant discriminants, and `EvmRuntimeConfigUpdate` keeps its original fields,
 * so
 * transactions created before custom precompiles existed still decode identically.
 */
export interface Evm {
    call?:                              Call;
    update_runtime_config?:             UpdateRuntimeConfig;
    update_enabled_custom_precompiles?: UpdateEnabledCustomPrecompiles;
}

/**
 * RLP encoded evm transaction.
 */
export interface Call {
    /**
     * Rlp data.
     */
    rlp: number[];
    [property: string]: any;
}

/**
 * An update to the enabled custom EVM precompile set.
 */
export interface UpdateEnabledCustomPrecompiles {
    /**
     * Custom precompile addresses to enable.
     */
    add: string[];
    /**
     * Custom precompile addresses to disable.
     */
    remove: string[];
    [property: string]: any;
}

/**
 * An update to the runtime configuration.
 */
export interface UpdateRuntimeConfig {
    /**
     * A new chain spec to apply. None means "no change"
     */
    chain_spec_update?: null | ChainSpecUpdateObject;
    /**
     * A new admin address to set. None means "no change"
     */
    new_admin?: null | string;
    /**
     * A new contract creation policy to apply. None means "no change"
     */
    new_contract_creation_policy?: NewContractCreationPolicyClass | NewContractCreationPolicyEnum | null;
    /**
     * A new hardfork to activate and the block number at which it activates
     */
    new_hardfork?: any[] | null;
    [property: string]: any;
}

/**
 * An update to the chain spec.
 */
export interface ChainSpecUpdateObject {
    /**
     * The new block gas limit. Must be greater than 5M to avoid censorship. None means "no
     * change"
     * Check that the limit is greater than 5M to avoid accidental complete shutdown.
     */
    new_block_gas_limit?: number | null;
    /**
     * The new limit for contract code size. None means "no change"
     */
    new_limit_contract_code_size?: number | null;
    /**
     * The new tx gas limit. Must be less than or equal to the effective block gas limit after
     * applying the update. None means "no change"
     */
    new_tx_gas_limit?: number | null;
    [property: string]: any;
}

/**
 * Only allowed addresses can create contracts
 */
export interface NewContractCreationPolicyClass {
    allowlist: Allowlist;
}

export interface Allowlist {
    /**
     * Addresses to add to the allowlist
     */
    add: string[];
    /**
     * Addresses to remove from the allowlist
     */
    remove: string[];
    [property: string]: any;
}

/**
 * No restrictions on contract creation
 */
export enum NewContractCreationPolicyEnum {
    Everyone = "everyone",
}

/**
 * InterchainGasPaymaster CallMessage
 *
 * Set or update config for relayer (sender).
 *
 * This could be used to clear values too.
 *
 * Update oracle data for relayer (sender)
 *
 * Beneficiary (sender) claim all relayer rewards.
 */
export interface InterchainGasPaymaster {
    set_relayer_config?: SetRelayerConfig;
    update_oracle_data?: UpdateOracleData;
    claim_rewards?:      ClaimRewards;
}

export interface ClaimRewards {
    /**
     * Relayer to transfer tokens from.
     */
    relayer_address: string;
    [property: string]: any;
}

export interface SetRelayerConfig {
    /**
     * Beneficiary who can claim relayer rewards.
     */
    beneficiary?: null | string;
    /**
     * Default gas used if custom one is not set.
     */
    default_gas: string;
    /**
     * Custom default gas per domain.
     */
    domain_default_gas: DomainDefaultGasElement[];
    /**
     * Oracle data per domain.
     */
    domain_oracle_data: DomainOracleDatumElement[];
    [property: string]: any;
}

/**
 * Custom default gas per domain.
 *
 * Domain Default Gas used in `CallMessage::SetRelayerConfig`.
 */
export interface DomainDefaultGasElement {
    /**
     * Default gas.
     */
    default_gas: string;
    /**
     * Domain.
     */
    domain: number;
    [property: string]: any;
}

/**
 * Oracle data per domain.
 *
 * Domain Oracle Data used in `CallMessage::SetRelayerConfig`.
 */
export interface DomainOracleDatumElement {
    /**
     * Oracle data value.
     */
    data_value: DataValue;
    /**
     * Domain.
     */
    domain: number;
    [property: string]: any;
}

/**
 * Oracle data value.
 *
 * Oracle data used to calculate required gas.
 *
 * Oracle data.
 *
 * Relayer is responsible to multiple token_rate_exchange by `TOKEN_EXCHANGE_RATE_SCALE`.
 */
export interface DataValue {
    /**
     * Gas price.
     */
    gas_price: string;
    /**
     * Token exchange rate, calculated as local gas token price / remote gas token price.
     *
     * Relayer is responsible to multiply token_rate_exchange by `TOKEN_EXCHANGE_RATE_SCALE`.
     */
    token_exchange_rate: number;
    [property: string]: any;
}

export interface UpdateOracleData {
    /**
     * Domain or destination domain (i.e. chain id in hyperlane).
     */
    domain: number;
    /**
     * Oracle data.
     *
     * Relayer is responsible to multiple token_rate_exchange by `TOKEN_EXCHANGE_RATE_SCALE`.
     */
    oracle_data: DataValue;
    [property: string]: any;
}

/**
 * This enumeration represents the available call messages for interacting with the
 * `sov-value-setter` module.
 *
 * Sends an outbound message to the specified recipient.
 *
 * Receive an inbound message. This is called *on the desitination chain* by the relayer
 * after
 * a `dispatch` call has been made on the source chain.
 *
 * Passes the message metadata and body to the security module (ISM) for verification,
 * then calls the recipient's `handle` function with the message body.
 *
 * Announce a validator and its signatures' storage.
 */
export interface Mailbox {
    dispatch?: Dispatch;
    process?:  Process;
    announce?: Announce;
}

export interface Announce {
    /**
     * Signature of the announcement message for verification.
     */
    signature: string;
    /**
     * Location of validator's signatures.
     */
    storage_location: string;
    /**
     * Address of a validator.
     */
    validator_address: string;
    [property: string]: any;
}

export interface Dispatch {
    /**
     * The message body. For example, if the recipient is a warp route, this will encode the
     * amount/type of funds being transferred
     */
    body: string;
    /**
     * The destination domain (aka "Chain ID")
     */
    domain: number;
    /**
     * A limit for the payment to relayer to cover gas needed for message delivery.
     * If relayer demands more than this value of native gas token, dispatching message
     * will fail. If it demands less than this, only needed amount will be paid.
     */
    gas_payment_limit: string;
    /**
     * The "metadata" which is used to verify the message or control hooks.
     * Can be used to set the destination gas limit for a message using
     * [`IGPMetadata`](crate::igp::IGPMetadata)
     */
    metadata?: null | string;
    /**
     * The recipient address. Must implement the `handle` function - i.e. be a smart contract
     */
    recipient: string;
    /**
     * Selected relayer
     */
    relayer?: null | string;
    [property: string]: any;
}

export interface Process {
    /**
     * The serialized [`Message`] struct
     */
    message: string;
    /**
     * Metadata used to verify the message.
     */
    metadata: string;
    [property: string]: any;
}

/**
 * This enumeration represents the available call messages for interacting with the
 * sov-operator-incentives module.
 */
export interface OperatorIncentives {
    update_reward_address: UpdateRewardAddress;
}

export interface UpdateRewardAddress {
    /**
     * The new address that will receive rewards for operating the rollup.
     * Note: We do not verify possession of the corresponding private key,
     * so it's possible to set an address for which the `sender` does not control the private
     * key.
     */
    new_reward_address: string;
    [property: string]: any;
}

/**
 * Call messages for interacting with the `Paymaster` module.
 *
 * ## Note:
 * These call messages are highly unusual in that they have different effects
 * based on the address of the sequencer who places them on chain. See the docs
 * on individual variants for more information.
 *
 * Register a new payer with the given policy. If the sequencer who places this message on
 * chain
 * is present in the list of `authorized_sequencers` to use the payer, the payer address for
 * that
 * sequencer is set to the address of the newly registered payer.
 *
 * Set the payer address for the sequencer to the given address.
 * This call message is highly unusual in that it executes regardless of the sender address
 * on the rollup.
 * Sequencers who do not wish to update their payer address should not sequence transactions
 * containing this callmessage.
 *
 * Update the policy for a given payer. If the sequencer who places this message on chain
 * is present in the list of `authorized_sequencers` to use the payer after the update, the
 * payer address for that
 * sequencer is set to the address of the newly registered paymaster.
 */
export interface Paymaster {
    register_paymaster?:      RegisterPaymaster;
    set_payer_for_sequencer?: SetPayerForSequencer;
    update_policy?:           UpdatePolicy;
}

export interface RegisterPaymaster {
    policy: Policy;
    [property: string]: any;
}

/**
 * An initial policy for a paymaster. This includes...
 * - A set of sequencers that can use the paymaster
 * - A set of users authorized to update this policy
 * - A default policy for accepting/rejecting gas requests
 * - Specific policies for accepting/rejecting gas requests from particular users
 */
export interface Policy {
    /**
     * Sequencers who are authorized to use this payer.
     */
    authorized_sequencers: AuthorizedSequencersClass | AuthorizedSequencersEnum;
    /**
     * Users who are authorized to update this policy.
     */
    authorized_updaters: string[];
    /**
     * Default payee policy for users that are not in the balances map.
     */
    default_payee_policy: DefaultPayeePolicyClass | DefaultPayeePolicyEnum;
    /**
     * A mapping from user address to the policy for that user.
     */
    payees: Array<any[]>;
}

/**
 * Only the specified sequencers may use this payer.
 */
export interface AuthorizedSequencersClass {
    some: string[];
}

/**
 * All sequencers are authorized to use this payer (according to its policy).
 */
export enum AuthorizedSequencersEnum {
    All = "all",
}

/**
 * The paymaster pays the fees for a particular sender when the policy allows it...
 * - If the policy specifies a `max_fee`, the transaction's max fee must be less than or
 * equal to that value
 * - if the policy specifies a `max_gas_price`, the current gas price must be less than or
 * equal to that value
 * - If the policy specifies a gas limit, the transaction must also specify a limit *and*
 * that limit must be less than or equal to `gas_limit`.
 *
 * - If the policy specifies a transaction_limit, the policy can only cover that many
 * transactions, after which it will expire and be replaced with a Deny policy
 *
 * In all other cases, the sender pays their own fees.
 */
export interface DefaultPayeePolicyClass {
    allow: Allow;
}

export interface Allow {
    gas_limit?:         number[] | null;
    max_fee?:           null | string;
    max_gas_price?:     number[] | null;
    transaction_limit?: number | null;
    [property: string]: any;
}

/**
 * The payer does not pay fees for any transaction using this policy.
 */
export enum DefaultPayeePolicyEnum {
    Deny = "deny",
}

export interface SetPayerForSequencer {
    payer: string;
    [property: string]: any;
}

export interface UpdatePolicy {
    payer:  string;
    update: UpdatePolicyUpdate;
    [property: string]: any;
}

/**
 * An update to the policy of a single gas payer
 */
export interface UpdatePolicyUpdate {
    default_policy?:           DefaultPayeePolicyClass | DefaultPayeePolicyEnum | null;
    payee_policies_to_delete?: string[] | null;
    payee_policies_to_set?:    Array<any[]> | null;
    sequencer_update?:         SequencerUpdateClass | SequencerUpdateEnum | null;
    updaters_to_add?:          string[] | null;
    updaters_to_remove?:       string[] | null;
    [property: string]: any;
}

/**
 * Sets the list of authorized sequencers to an explicit whitelist if it was previously
 * `AllowAll`.
 * Adds and removes the requested addresses from the sequencer whitelist.
 */
export interface SequencerUpdateClass {
    update: SequencerUpdateUpdate;
}

/**
 * A list of updates to the `allowed_sequencers` list for a particular payer.
 */
export interface SequencerUpdateUpdate {
    to_add?:    string[] | null;
    to_remove?: string[] | null;
    [property: string]: any;
}

/**
 * Authorizes any sequencer to use this payer.
 */
export enum SequencerUpdateEnum {
    AllowAll = "allow_all",
}

/**
 * Add a new prover as a bonded prover.
 *
 * Increases the balance of the prover, transferring the funds from the prover account
 * to the rollup.
 */
export interface ProverIncentivesClass {
    register?: string;
    deposit?:  string;
}

/**
 * Unbonds the prover.
 */
export enum ProverIncentivesEnum {
    Exit = "exit",
}

/**
 * Lower the revenue share percentage (admin only)
 *
 * Update the sovereign admin address (current admin only)
 *
 * Withdraw accumulated rewards to the admin address (admin only)
 */
export interface RevenueShareClass {
    lower_revenue_percentage?: LowerRevenuePercentage;
    update_sovereign_admin?:   UpdateSovereignAdmin;
    withdraw_rewards?:         WithdrawRewards;
}

export interface LowerRevenuePercentage {
    /**
     * New percentage in basis points (e.g., 1000 = 10%)
     */
    percentage_in_basis_points: number;
    [property: string]: any;
}

export interface UpdateSovereignAdmin {
    /**
     * The new admin address
     */
    new_admin: string;
    [property: string]: any;
}

export interface WithdrawRewards {
    /**
     * The token ID to withdraw
     */
    token_id: string;
    [property: string]: any;
}

/**
 * Activate revenue sharing (admin only)
 *
 * Deactivate revenue sharing (admin only)
 */
export enum RevenueShareEnum {
    ActivateRevenueShare = "activate_revenue_share",
    DeactivateRevenueShare = "deactivate_revenue_share",
}

/**
 * This enumeration represents the available call messages for interacting with
 * the `sov-sequencer-registry` module.
 *
 * Add a new sequencer to the sequencer registry.
 *
 * Increases the balance of the sequencer, transferring the funds from the sequencer account
 * to the rollup.
 *
 * Initiate a withdrawal of a sequencer's balance.
 *
 * Withdraw a sequencer's balance after waiting for the withdrawal period.
 */
export interface SequencerRegistry {
    register?:            SequencerRegistryRegister;
    deposit?:             Deposit;
    initiate_withdrawal?: InitiateWithdrawal;
    withdraw?:            Withdraw;
}

export interface Deposit {
    /**
     * The amount to increase.
     */
    amount: string;
    /**
     * The DA address of the sequencer.
     */
    da_address: string;
    [property: string]: any;
}

export interface InitiateWithdrawal {
    /**
     * The DA address of the sequencer you're removing.
     */
    da_address: string;
    [property: string]: any;
}

export interface SequencerRegistryRegister {
    /**
     * The initial balance of the sequencer.
     */
    amount: string;
    /**
     * The Da address of the sequencer you're registering.
     */
    da_address: string;
    [property: string]: any;
}

export interface Withdraw {
    /**
     * The DA address of the sequencer you're removing.
     */
    da_address: string;
    [property: string]: any;
}

export interface ValueSetter {
    set_value: number;
}

/**
 * Call messages for the test recipient module.
 *
 * Register a route with the given token source and ISM.
 *
 * Warp routes should be rate-limited for security purposes. Its main purpose is to prevent
 * draining a warp
 * route in case an exploit is discovered or validators are malicious. Hyperlane uses "token
 * bucket"
 * algorithm for rate limitting. If rate limitting is not desired, the parameters that
 * control
 * it can be set to [`Amount::MAX`].
 *
 * Update an existing route with new admin or ISM.
 *
 * Add a counterparty router on another chain. This router is trusted. A malicious remote
 * router can steal funds.
 * Each warp route can have at most one remote router for a given destination domain.
 *
 * Remove a counterparty router on another chain.
 *
 * Transfer a token from the local chain to the remote chain.
 */
export interface Warp {
    register?:                WarpRegister;
    update?:                  WarpUpdate;
    enroll_remote_router?:    EnrollRemoteRouter;
    un_enroll_remote_router?: UnEnrollRemoteRouter;
    transfer_remote?:         TransferRemote;
}

export interface EnrollRemoteRouter {
    /**
     * The domain of the remote chain.
     */
    remote_domain: number;
    /**
     * The router address on the remote chain.
     */
    remote_router_address: string;
    /**
     * The ID of the warp route on the local chain.
     */
    warp_route: string;
    [property: string]: any;
}

export interface WarpRegister {
    /**
     * The authority that can modify the route, if any.
     */
    admin: AdminClass | AdminEnum;
    /**
     * Defines how many tokens are added to the inbound transferrable limit at each visible
     * slot.
     *
     * Usually this should be set to a value which allows full limit recovery each day,
     * so `transferrable_tokens_limit / DA_SLOTS_PER_DAY`.
     */
    inbound_limit_replenishment_per_slot: string;
    /**
     * Defines how many tokens can be transferred to the route without any refills.
     *
     * No more than this amount can be transferred in a single slot. Each transfer decreases
     * current limit value until 0, at which point all new transfers are rejected.
     * The limit recovers at each visible slot by a configured amount.
     */
    inbound_transferrable_tokens_limit: string;
    /**
     * The ISM for this route.
     */
    ism: PurpleJSONSchema | IsmEnum;
    /**
     * Defines how many tokens are added to the outbound transferrable limit at each visible
     * slot.
     *
     * Usually this should be set to a value which allows full limit recovery each day,
     * so `transferrable_tokens_limit / DA_SLOTS_PER_DAY`.
     */
    outbound_limit_replenishment_per_slot: string;
    /**
     * Defines how many tokens can be transferred from the route without any refills.
     *
     * No more than this amount can be transferred in a single slot. Each transfer decreases
     * current limit value until 0, at which point all new transfers are rejected.
     * The limit recovers at each visible slot by a configured amount.
     */
    outbound_transferrable_tokens_limit: string;
    /**
     * Remote routers to enroll on route registration.
     */
    remote_routers: Array<any[]>;
    /**
     * The token source for the route.
     */
    token_source: TokenSourceClass | TokenSourceEnum;
    [property: string]: any;
}

/**
 * Allow the specified address to modify the route. This is extremely insecure,
 * but it seems to be common practice in Hyperlane.
 */
export interface AdminClass {
    InsecureOwner: string;
}

/**
 * No admin - the route is immutable.
 */
export enum AdminEnum {
    None = "None",
}

/**
 * Accepts all messages from a trusted relayer
 *
 * Accepts messages if signed by `threshold` or more of the provided `validators`
 */
export interface PurpleJSONSchema {
    TrustedRelayer?:    TrustedRelayer;
    MessageIdMultisig?: MessageIDMultisig;
}

export interface MessageIDMultisig {
    /**
     * The number of signatures required to accept a message
     */
    threshold: number;
    /**
     * The addresses of the validators
     */
    validators: string[];
    [property: string]: any;
}

export interface TrustedRelayer {
    /**
     * The address of the trusted relayer, in [`HyperlaneAddress`] format
     */
    relayer: string;
    [property: string]: any;
}

/**
 * Performs no validation. Will accept any message - useful for testing
 */
export enum IsmEnum {
    AlwaysTrust = "AlwaysTrust",
}

/**
 * The token is natively issued on some remote chain, so the local representation is a
 * synthetic token.
 *
 * The token is natively issued on the local chain.
 */
export interface TokenSourceClass {
    Synthetic?:  Synthetic;
    Collateral?: Collateral;
}

export interface Collateral {
    /**
     * The ID of the token on the local chain.
     */
    token: string;
    [property: string]: any;
}

export interface Synthetic {
    /**
     * The number of decimal places for the local (synthetic) token.
     *
     * Should be set if remote token should be scaled locally, defaults to remote decimals.
     */
    local_decimals?: number | null;
    /**
     * The number of decimal places of the remote token.
     */
    remote_decimals: number;
    /**
     * The ID of the remote token.
     */
    remote_token_id: string;
    [property: string]: any;
}

/**
 * The token is the native token of the local chain.
 */
export enum TokenSourceEnum {
    Native = "Native",
}

export interface TransferRemote {
    /**
     * The amount to transfer.
     */
    amount: string;
    /**
     * The domain of the destination chain.
     */
    destination_domain: number;
    /**
     * A limit for the payment to relayer to cover gas needed for message delivery.
     */
    gas_payment_limit: string;
    /**
     * The recipient on the destination chain.
     */
    recipient: string;
    /**
     * Selected relayer
     */
    relayer?: null | string;
    /**
     * The route to use for the transfer.
     */
    warp_route: string;
    [property: string]: any;
}

export interface UnEnrollRemoteRouter {
    /**
     * The domain of the remote chain.
     */
    remote_domain: number;
    /**
     * The ID of the warp route on the local chain.
     */
    warp_route: string;
    [property: string]: any;
}

export interface WarpUpdate {
    /**
     * New authority that can modify the route.
     */
    admin?: AdminClass | AdminEnum | null;
    /**
     * Defines how many tokens are added to the inbound transferrable limit at each visible slot.
     */
    inbound_limit_replenishment_per_slot?: null | string;
    /**
     * Defines how many tokens can be transferred to the route without any refills.
     */
    inbound_transferrable_tokens_limit?: null | string;
    /**
     * New ISM for this route.
     */
    ism?: FluffyJSONSchema | IsmEnum | null;
    /**
     * Defines how many tokens are added to the outbound transferrable limit at each visible
     * slot.
     */
    outbound_limit_replenishment_per_slot?: null | string;
    /**
     * Defines how many tokens can be transferred from the route without any refills.
     */
    outbound_transferrable_tokens_limit?: null | string;
    /**
     * The ID of the warp route on the local chain to update.
     */
    warp_route: string;
    [property: string]: any;
}

/**
 * Accepts all messages from a trusted relayer
 *
 * Accepts messages if signed by `threshold` or more of the provided `validators`
 */
export interface FluffyJSONSchema {
    TrustedRelayer?:    TrustedRelayer;
    MessageIdMultisig?: MessageIDMultisig;
}

// Converts JSON strings to/from your types
// and asserts the results of JSON.parse at runtime
export class Convert {
    public static toRuntimeCall(json: string): RuntimeCall {
        return cast(JSON.parse(json), r("RuntimeCall"));
    }

    public static runtimeCallToJson(value: RuntimeCall): string {
        return JSON.stringify(uncast(value, r("RuntimeCall")), null, 2);
    }
}

function invalidValue(typ: any, val: any, key: any, parent: any = ''): never {
    const prettyTyp = prettyTypeName(typ);
    const parentText = parent ? ` on ${parent}` : '';
    const keyText = key ? ` for key "${key}"` : '';
    throw Error(`Invalid value${keyText}${parentText}. Expected ${prettyTyp} but got ${JSON.stringify(val)}`);
}

function prettyTypeName(typ: any): string {
    if (Array.isArray(typ)) {
        if (typ.length === 2 && typ[0] === undefined) {
            return `an optional ${prettyTypeName(typ[1])}`;
        } else {
            return `one of [${typ.map(a => { return prettyTypeName(a); }).join(", ")}]`;
        }
    } else if (typeof typ === "object" && typ.literal !== undefined) {
        return typ.literal;
    } else {
        return typeof typ;
    }
}

function jsonToJSProps(typ: any): any {
    if (typ.jsonToJS === undefined) {
        const map: any = {};
        typ.props.forEach((p: any) => map[p.json] = { key: p.js, typ: p.typ });
        typ.jsonToJS = map;
    }
    return typ.jsonToJS;
}

function jsToJSONProps(typ: any): any {
    if (typ.jsToJSON === undefined) {
        const map: any = {};
        typ.props.forEach((p: any) => map[p.js] = { key: p.json, typ: p.typ });
        typ.jsToJSON = map;
    }
    return typ.jsToJSON;
}

function transform(val: any, typ: any, getProps: any, key: any = '', parent: any = ''): any {
    function transformPrimitive(typ: string, val: any): any {
        if (typeof typ === typeof val) return val;
        return invalidValue(typ, val, key, parent);
    }

    function transformUnion(typs: any[], val: any): any {
        // val must validate against one typ in typs
        const l = typs.length;
        for (let i = 0; i < l; i++) {
            const typ = typs[i];
            try {
                return transform(val, typ, getProps);
            } catch (_) {}
        }
        return invalidValue(typs, val, key, parent);
    }

    function transformEnum(cases: string[], val: any): any {
        if (cases.indexOf(val) !== -1) return val;
        return invalidValue(cases.map(a => { return l(a); }), val, key, parent);
    }

    function transformArray(typ: any, val: any): any {
        // val must be an array with no invalid elements
        if (!Array.isArray(val)) return invalidValue(l("array"), val, key, parent);
        return val.map(el => transform(el, typ, getProps));
    }

    function transformDate(val: any): any {
        if (val === null) {
            return null;
        }
        const d = new Date(val);
        if (isNaN(d.valueOf())) {
            return invalidValue(l("Date"), val, key, parent);
        }
        return d;
    }

    function transformObject(props: { [k: string]: any }, additional: any, val: any): any {
        if (val === null || typeof val !== "object" || Array.isArray(val)) {
            return invalidValue(l(ref || "object"), val, key, parent);
        }
        const result: any = {};
        Object.getOwnPropertyNames(props).forEach(key => {
            const prop = props[key];
            const v = Object.prototype.hasOwnProperty.call(val, key) ? val[key] : undefined;
            result[prop.key] = transform(v, prop.typ, getProps, key, ref);
        });
        Object.getOwnPropertyNames(val).forEach(key => {
            if (!Object.prototype.hasOwnProperty.call(props, key)) {
                result[key] = transform(val[key], additional, getProps, key, ref);
            }
        });
        return result;
    }

    if (typ === "any") return val;
    if (typ === null) {
        if (val === null) return val;
        return invalidValue(typ, val, key, parent);
    }
    if (typ === false) return invalidValue(typ, val, key, parent);
    let ref: any = undefined;
    while (typeof typ === "object" && typ.ref !== undefined) {
        ref = typ.ref;
        typ = typeMap[typ.ref];
    }
    if (Array.isArray(typ)) return transformEnum(typ, val);
    if (typeof typ === "object") {
        return typ.hasOwnProperty("unionMembers") ? transformUnion(typ.unionMembers, val)
            : typ.hasOwnProperty("arrayItems")    ? transformArray(typ.arrayItems, val)
            : typ.hasOwnProperty("props")         ? transformObject(getProps(typ), typ.additional, val)
            : invalidValue(typ, val, key, parent);
    }
    // Numbers can be parsed by Date but shouldn't be.
    if (typ === Date && typeof val !== "number") return transformDate(val);
    return transformPrimitive(typ, val);
}

function cast<T>(val: any, typ: any): T {
    return transform(val, typ, jsonToJSProps);
}

function uncast<T>(val: T, typ: any): any {
    return transform(val, typ, jsToJSONProps);
}

function l(typ: any) {
    return { literal: typ };
}

function a(typ: any) {
    return { arrayItems: typ };
}

function u(...typs: any[]) {
    return { unionMembers: typs };
}

function o(props: any[], additional: any) {
    return { props, additional };
}

function m(additional: any) {
    return { props: [], additional };
}

function r(name: string) {
    return { ref: name };
}

const typeMap: any = {
    "RuntimeCall": o([
        { json: "accounts", js: "accounts", typ: u(undefined, r("Accounts")) },
        { json: "uniqueness", js: "uniqueness", typ: u(undefined, null) },
        { json: "bank", js: "bank", typ: u(undefined, r("Bank")) },
        { json: "sequencer_registry", js: "sequencer_registry", typ: u(undefined, r("SequencerRegistry")) },
        { json: "operator_incentives", js: "operator_incentives", typ: u(undefined, r("OperatorIncentives")) },
        { json: "attester_incentives", js: "attester_incentives", typ: u(undefined, u(r("AttesterIncentivesClass"), r("AttesterIncentivesEnum"))) },
        { json: "prover_incentives", js: "prover_incentives", typ: u(undefined, u(r("ProverIncentivesClass"), r("ProverIncentivesEnum"))) },
        { json: "chain_state", js: "chain_state", typ: u(undefined, u(r("ChainStateClass"), r("ChainStateEnum"))) },
        { json: "blob_storage", js: "blob_storage", typ: u(undefined, null) },
        { json: "paymaster", js: "paymaster", typ: u(undefined, r("Paymaster")) },
        { json: "revenue_share", js: "revenue_share", typ: u(undefined, u(r("RevenueShareClass"), r("RevenueShareEnum"))) },
        { json: "mailbox", js: "mailbox", typ: u(undefined, r("Mailbox")) },
        { json: "interchain_gas_paymaster", js: "interchain_gas_paymaster", typ: u(undefined, r("InterchainGasPaymaster")) },
        { json: "merkle_tree_hook", js: "merkle_tree_hook", typ: u(undefined, null) },
        { json: "warp", js: "warp", typ: u(undefined, r("Warp")) },
        { json: "value_setter", js: "value_setter", typ: u(undefined, r("ValueSetter")) },
        { json: "evm", js: "evm", typ: u(undefined, r("Evm")) },
    ], false),
    "Accounts": o([
        { json: "insert_credential_id", js: "insert_credential_id", typ: u(undefined, "") },
        { json: "add_credential_to_address", js: "add_credential_to_address", typ: u(undefined, r("AddCredentialToAddress")) },
        { json: "remove_credential_from_address", js: "remove_credential_from_address", typ: u(undefined, r("RemoveCredentialFromAddress")) },
        { json: "rotate_credential_on_address", js: "rotate_credential_on_address", typ: u(undefined, r("RotateCredentialOnAddress")) },
        { json: "create_synthetic_address", js: "create_synthetic_address", typ: u(undefined, r("CreateSyntheticAddress")) },
    ], false),
    "AddCredentialToAddress": o([
        { json: "address", js: "address", typ: "" },
        { json: "credential", js: "credential", typ: "" },
    ], "any"),
    "CreateSyntheticAddress": o([
        { json: "salt", js: "salt", typ: a(0) },
    ], "any"),
    "RemoveCredentialFromAddress": o([
        { json: "address", js: "address", typ: "" },
        { json: "credential", js: "credential", typ: "" },
    ], "any"),
    "RotateCredentialOnAddress": o([
        { json: "address", js: "address", typ: "" },
        { json: "new_credential", js: "new_credential", typ: "" },
        { json: "old_credential", js: "old_credential", typ: "" },
    ], "any"),
    "AttesterIncentivesClass": o([
        { json: "register_attester", js: "register_attester", typ: u(undefined, "") },
        { json: "register_challenger", js: "register_challenger", typ: u(undefined, "") },
        { json: "deposit_attester", js: "deposit_attester", typ: u(undefined, "") },
    ], false),
    "Bank": o([
        { json: "create_token", js: "create_token", typ: u(undefined, r("CreateToken")) },
        { json: "transfer", js: "transfer", typ: u(undefined, r("Transfer")) },
        { json: "burn", js: "burn", typ: u(undefined, r("Burn")) },
        { json: "mint", js: "mint", typ: u(undefined, r("Mint")) },
        { json: "freeze", js: "freeze", typ: u(undefined, r("Freeze")) },
        { json: "update_admin", js: "update_admin", typ: u(undefined, r("UpdateAdmin")) },
        { json: "transfer_with_memo", js: "transfer_with_memo", typ: u(undefined, r("TransferWithMemo")) },
    ], false),
    "Burn": o([
        { json: "coins", js: "coins", typ: r("Coins") },
    ], "any"),
    "Coins": o([
        { json: "amount", js: "amount", typ: "" },
        { json: "token_id", js: "token_id", typ: "" },
    ], "any"),
    "CreateToken": o([
        { json: "admins", js: "admins", typ: a("") },
        { json: "initial_balance", js: "initial_balance", typ: "" },
        { json: "mint_to_address", js: "mint_to_address", typ: "" },
        { json: "supply_cap", js: "supply_cap", typ: u(undefined, u(null, "")) },
        { json: "token_decimals", js: "token_decimals", typ: u(undefined, u(0, null)) },
        { json: "token_name", js: "token_name", typ: "" },
    ], "any"),
    "Freeze": o([
        { json: "token_id", js: "token_id", typ: "" },
    ], "any"),
    "Mint": o([
        { json: "coins", js: "coins", typ: r("Coins") },
        { json: "mint_to_address", js: "mint_to_address", typ: "" },
    ], "any"),
    "Transfer": o([
        { json: "coins", js: "coins", typ: r("Coins") },
        { json: "to", js: "to", typ: "" },
    ], "any"),
    "TransferWithMemo": o([
        { json: "coins", js: "coins", typ: r("Coins") },
        { json: "memo", js: "memo", typ: "" },
        { json: "to", js: "to", typ: "" },
    ], "any"),
    "UpdateAdmin": o([
        { json: "new_admin", js: "new_admin", typ: u(undefined, u(null, "")) },
        { json: "token_id", js: "token_id", typ: "" },
    ], "any"),
    "ChainStateClass": o([
        { json: "SetOracleTime", js: "SetOracleTime", typ: r("SetOracleTime") },
    ], false),
    "SetOracleTime": o([
        { json: "milliseconds_since_epoch", js: "milliseconds_since_epoch", typ: 0 },
    ], "any"),
    "Evm": o([
        { json: "call", js: "call", typ: u(undefined, r("Call")) },
        { json: "update_runtime_config", js: "update_runtime_config", typ: u(undefined, r("UpdateRuntimeConfig")) },
        { json: "update_enabled_custom_precompiles", js: "update_enabled_custom_precompiles", typ: u(undefined, r("UpdateEnabledCustomPrecompiles")) },
    ], false),
    "Call": o([
        { json: "rlp", js: "rlp", typ: a(0) },
    ], "any"),
    "UpdateEnabledCustomPrecompiles": o([
        { json: "add", js: "add", typ: a("") },
        { json: "remove", js: "remove", typ: a("") },
    ], "any"),
    "UpdateRuntimeConfig": o([
        { json: "chain_spec_update", js: "chain_spec_update", typ: u(undefined, u(null, r("ChainSpecUpdateObject"))) },
        { json: "new_admin", js: "new_admin", typ: u(undefined, u(null, "")) },
        { json: "new_contract_creation_policy", js: "new_contract_creation_policy", typ: u(undefined, u(r("NewContractCreationPolicyClass"), r("NewContractCreationPolicyEnum"), null)) },
        { json: "new_hardfork", js: "new_hardfork", typ: u(undefined, u(a("any"), null)) },
    ], "any"),
    "ChainSpecUpdateObject": o([
        { json: "new_block_gas_limit", js: "new_block_gas_limit", typ: u(undefined, u(0, null)) },
        { json: "new_limit_contract_code_size", js: "new_limit_contract_code_size", typ: u(undefined, u(0, null)) },
        { json: "new_tx_gas_limit", js: "new_tx_gas_limit", typ: u(undefined, u(0, null)) },
    ], "any"),
    "NewContractCreationPolicyClass": o([
        { json: "allowlist", js: "allowlist", typ: r("Allowlist") },
    ], false),
    "Allowlist": o([
        { json: "add", js: "add", typ: a("") },
        { json: "remove", js: "remove", typ: a("") },
    ], "any"),
    "InterchainGasPaymaster": o([
        { json: "set_relayer_config", js: "set_relayer_config", typ: u(undefined, r("SetRelayerConfig")) },
        { json: "update_oracle_data", js: "update_oracle_data", typ: u(undefined, r("UpdateOracleData")) },
        { json: "claim_rewards", js: "claim_rewards", typ: u(undefined, r("ClaimRewards")) },
    ], false),
    "ClaimRewards": o([
        { json: "relayer_address", js: "relayer_address", typ: "" },
    ], "any"),
    "SetRelayerConfig": o([
        { json: "beneficiary", js: "beneficiary", typ: u(undefined, u(null, "")) },
        { json: "default_gas", js: "default_gas", typ: "" },
        { json: "domain_default_gas", js: "domain_default_gas", typ: a(r("DomainDefaultGasElement")) },
        { json: "domain_oracle_data", js: "domain_oracle_data", typ: a(r("DomainOracleDatumElement")) },
    ], "any"),
    "DomainDefaultGasElement": o([
        { json: "default_gas", js: "default_gas", typ: "" },
        { json: "domain", js: "domain", typ: 0 },
    ], "any"),
    "DomainOracleDatumElement": o([
        { json: "data_value", js: "data_value", typ: r("DataValue") },
        { json: "domain", js: "domain", typ: 0 },
    ], "any"),
    "DataValue": o([
        { json: "gas_price", js: "gas_price", typ: "" },
        { json: "token_exchange_rate", js: "token_exchange_rate", typ: 0 },
    ], "any"),
    "UpdateOracleData": o([
        { json: "domain", js: "domain", typ: 0 },
        { json: "oracle_data", js: "oracle_data", typ: r("DataValue") },
    ], "any"),
    "Mailbox": o([
        { json: "dispatch", js: "dispatch", typ: u(undefined, r("Dispatch")) },
        { json: "process", js: "process", typ: u(undefined, r("Process")) },
        { json: "announce", js: "announce", typ: u(undefined, r("Announce")) },
    ], false),
    "Announce": o([
        { json: "signature", js: "signature", typ: "" },
        { json: "storage_location", js: "storage_location", typ: "" },
        { json: "validator_address", js: "validator_address", typ: "" },
    ], "any"),
    "Dispatch": o([
        { json: "body", js: "body", typ: "" },
        { json: "domain", js: "domain", typ: 0 },
        { json: "gas_payment_limit", js: "gas_payment_limit", typ: "" },
        { json: "metadata", js: "metadata", typ: u(undefined, u(null, "")) },
        { json: "recipient", js: "recipient", typ: "" },
        { json: "relayer", js: "relayer", typ: u(undefined, u(null, "")) },
    ], "any"),
    "Process": o([
        { json: "message", js: "message", typ: "" },
        { json: "metadata", js: "metadata", typ: "" },
    ], "any"),
    "OperatorIncentives": o([
        { json: "update_reward_address", js: "update_reward_address", typ: r("UpdateRewardAddress") },
    ], false),
    "UpdateRewardAddress": o([
        { json: "new_reward_address", js: "new_reward_address", typ: "" },
    ], "any"),
    "Paymaster": o([
        { json: "register_paymaster", js: "register_paymaster", typ: u(undefined, r("RegisterPaymaster")) },
        { json: "set_payer_for_sequencer", js: "set_payer_for_sequencer", typ: u(undefined, r("SetPayerForSequencer")) },
        { json: "update_policy", js: "update_policy", typ: u(undefined, r("UpdatePolicy")) },
    ], false),
    "RegisterPaymaster": o([
        { json: "policy", js: "policy", typ: r("Policy") },
    ], "any"),
    "Policy": o([
        { json: "authorized_sequencers", js: "authorized_sequencers", typ: u(r("AuthorizedSequencersClass"), r("AuthorizedSequencersEnum")) },
        { json: "authorized_updaters", js: "authorized_updaters", typ: a("") },
        { json: "default_payee_policy", js: "default_payee_policy", typ: u(r("DefaultPayeePolicyClass"), r("DefaultPayeePolicyEnum")) },
        { json: "payees", js: "payees", typ: a(a("any")) },
    ], false),
    "AuthorizedSequencersClass": o([
        { json: "some", js: "some", typ: a("") },
    ], false),
    "DefaultPayeePolicyClass": o([
        { json: "allow", js: "allow", typ: r("Allow") },
    ], false),
    "Allow": o([
        { json: "gas_limit", js: "gas_limit", typ: u(undefined, u(a(3.14), null)) },
        { json: "max_fee", js: "max_fee", typ: u(undefined, u(null, "")) },
        { json: "max_gas_price", js: "max_gas_price", typ: u(undefined, u(a(3.14), null)) },
        { json: "transaction_limit", js: "transaction_limit", typ: u(undefined, u(0, null)) },
    ], "any"),
    "SetPayerForSequencer": o([
        { json: "payer", js: "payer", typ: "" },
    ], "any"),
    "UpdatePolicy": o([
        { json: "payer", js: "payer", typ: "" },
        { json: "update", js: "update", typ: r("UpdatePolicyUpdate") },
    ], "any"),
    "UpdatePolicyUpdate": o([
        { json: "default_policy", js: "default_policy", typ: u(undefined, u(r("DefaultPayeePolicyClass"), r("DefaultPayeePolicyEnum"), null)) },
        { json: "payee_policies_to_delete", js: "payee_policies_to_delete", typ: u(undefined, u(a(""), null)) },
        { json: "payee_policies_to_set", js: "payee_policies_to_set", typ: u(undefined, u(a(a("any")), null)) },
        { json: "sequencer_update", js: "sequencer_update", typ: u(undefined, u(r("SequencerUpdateClass"), r("SequencerUpdateEnum"), null)) },
        { json: "updaters_to_add", js: "updaters_to_add", typ: u(undefined, u(a(""), null)) },
        { json: "updaters_to_remove", js: "updaters_to_remove", typ: u(undefined, u(a(""), null)) },
    ], "any"),
    "SequencerUpdateClass": o([
        { json: "update", js: "update", typ: r("SequencerUpdateUpdate") },
    ], false),
    "SequencerUpdateUpdate": o([
        { json: "to_add", js: "to_add", typ: u(undefined, u(a(""), null)) },
        { json: "to_remove", js: "to_remove", typ: u(undefined, u(a(""), null)) },
    ], "any"),
    "ProverIncentivesClass": o([
        { json: "register", js: "register", typ: u(undefined, "") },
        { json: "deposit", js: "deposit", typ: u(undefined, "") },
    ], false),
    "RevenueShareClass": o([
        { json: "lower_revenue_percentage", js: "lower_revenue_percentage", typ: u(undefined, r("LowerRevenuePercentage")) },
        { json: "update_sovereign_admin", js: "update_sovereign_admin", typ: u(undefined, r("UpdateSovereignAdmin")) },
        { json: "withdraw_rewards", js: "withdraw_rewards", typ: u(undefined, r("WithdrawRewards")) },
    ], false),
    "LowerRevenuePercentage": o([
        { json: "percentage_in_basis_points", js: "percentage_in_basis_points", typ: 0 },
    ], "any"),
    "UpdateSovereignAdmin": o([
        { json: "new_admin", js: "new_admin", typ: "" },
    ], "any"),
    "WithdrawRewards": o([
        { json: "token_id", js: "token_id", typ: "" },
    ], "any"),
    "SequencerRegistry": o([
        { json: "register", js: "register", typ: u(undefined, r("SequencerRegistryRegister")) },
        { json: "deposit", js: "deposit", typ: u(undefined, r("Deposit")) },
        { json: "initiate_withdrawal", js: "initiate_withdrawal", typ: u(undefined, r("InitiateWithdrawal")) },
        { json: "withdraw", js: "withdraw", typ: u(undefined, r("Withdraw")) },
    ], false),
    "Deposit": o([
        { json: "amount", js: "amount", typ: "" },
        { json: "da_address", js: "da_address", typ: "" },
    ], "any"),
    "InitiateWithdrawal": o([
        { json: "da_address", js: "da_address", typ: "" },
    ], "any"),
    "SequencerRegistryRegister": o([
        { json: "amount", js: "amount", typ: "" },
        { json: "da_address", js: "da_address", typ: "" },
    ], "any"),
    "Withdraw": o([
        { json: "da_address", js: "da_address", typ: "" },
    ], "any"),
    "ValueSetter": o([
        { json: "set_value", js: "set_value", typ: 0 },
    ], false),
    "Warp": o([
        { json: "register", js: "register", typ: u(undefined, r("WarpRegister")) },
        { json: "update", js: "update", typ: u(undefined, r("WarpUpdate")) },
        { json: "enroll_remote_router", js: "enroll_remote_router", typ: u(undefined, r("EnrollRemoteRouter")) },
        { json: "un_enroll_remote_router", js: "un_enroll_remote_router", typ: u(undefined, r("UnEnrollRemoteRouter")) },
        { json: "transfer_remote", js: "transfer_remote", typ: u(undefined, r("TransferRemote")) },
    ], false),
    "EnrollRemoteRouter": o([
        { json: "remote_domain", js: "remote_domain", typ: 0 },
        { json: "remote_router_address", js: "remote_router_address", typ: "" },
        { json: "warp_route", js: "warp_route", typ: "" },
    ], "any"),
    "WarpRegister": o([
        { json: "admin", js: "admin", typ: u(r("AdminClass"), r("AdminEnum")) },
        { json: "inbound_limit_replenishment_per_slot", js: "inbound_limit_replenishment_per_slot", typ: "" },
        { json: "inbound_transferrable_tokens_limit", js: "inbound_transferrable_tokens_limit", typ: "" },
        { json: "ism", js: "ism", typ: u(r("PurpleJSONSchema"), r("IsmEnum")) },
        { json: "outbound_limit_replenishment_per_slot", js: "outbound_limit_replenishment_per_slot", typ: "" },
        { json: "outbound_transferrable_tokens_limit", js: "outbound_transferrable_tokens_limit", typ: "" },
        { json: "remote_routers", js: "remote_routers", typ: a(a("any")) },
        { json: "token_source", js: "token_source", typ: u(r("TokenSourceClass"), r("TokenSourceEnum")) },
    ], "any"),
    "AdminClass": o([
        { json: "InsecureOwner", js: "InsecureOwner", typ: "" },
    ], false),
    "PurpleJSONSchema": o([
        { json: "TrustedRelayer", js: "TrustedRelayer", typ: u(undefined, r("TrustedRelayer")) },
        { json: "MessageIdMultisig", js: "MessageIdMultisig", typ: u(undefined, r("MessageIDMultisig")) },
    ], false),
    "MessageIDMultisig": o([
        { json: "threshold", js: "threshold", typ: 0 },
        { json: "validators", js: "validators", typ: a("") },
    ], "any"),
    "TrustedRelayer": o([
        { json: "relayer", js: "relayer", typ: "" },
    ], "any"),
    "TokenSourceClass": o([
        { json: "Synthetic", js: "Synthetic", typ: u(undefined, r("Synthetic")) },
        { json: "Collateral", js: "Collateral", typ: u(undefined, r("Collateral")) },
    ], false),
    "Collateral": o([
        { json: "token", js: "token", typ: "" },
    ], "any"),
    "Synthetic": o([
        { json: "local_decimals", js: "local_decimals", typ: u(undefined, u(0, null)) },
        { json: "remote_decimals", js: "remote_decimals", typ: 0 },
        { json: "remote_token_id", js: "remote_token_id", typ: "" },
    ], "any"),
    "TransferRemote": o([
        { json: "amount", js: "amount", typ: "" },
        { json: "destination_domain", js: "destination_domain", typ: 0 },
        { json: "gas_payment_limit", js: "gas_payment_limit", typ: "" },
        { json: "recipient", js: "recipient", typ: "" },
        { json: "relayer", js: "relayer", typ: u(undefined, u(null, "")) },
        { json: "warp_route", js: "warp_route", typ: "" },
    ], "any"),
    "UnEnrollRemoteRouter": o([
        { json: "remote_domain", js: "remote_domain", typ: 0 },
        { json: "warp_route", js: "warp_route", typ: "" },
    ], "any"),
    "WarpUpdate": o([
        { json: "admin", js: "admin", typ: u(undefined, u(r("AdminClass"), r("AdminEnum"), null)) },
        { json: "inbound_limit_replenishment_per_slot", js: "inbound_limit_replenishment_per_slot", typ: u(undefined, u(null, "")) },
        { json: "inbound_transferrable_tokens_limit", js: "inbound_transferrable_tokens_limit", typ: u(undefined, u(null, "")) },
        { json: "ism", js: "ism", typ: u(undefined, u(r("FluffyJSONSchema"), r("IsmEnum"), null)) },
        { json: "outbound_limit_replenishment_per_slot", js: "outbound_limit_replenishment_per_slot", typ: u(undefined, u(null, "")) },
        { json: "outbound_transferrable_tokens_limit", js: "outbound_transferrable_tokens_limit", typ: u(undefined, u(null, "")) },
        { json: "warp_route", js: "warp_route", typ: "" },
    ], "any"),
    "FluffyJSONSchema": o([
        { json: "TrustedRelayer", js: "TrustedRelayer", typ: u(undefined, r("TrustedRelayer")) },
        { json: "MessageIdMultisig", js: "MessageIdMultisig", typ: u(undefined, r("MessageIDMultisig")) },
    ], false),
    "AttesterIncentivesEnum": [
        "begin_exit_attester",
        "exit_attester",
        "exit_challenger",
    ],
    "ChainStateEnum": [
        "TerminateSetupMode",
    ],
    "NewContractCreationPolicyEnum": [
        "everyone",
    ],
    "AuthorizedSequencersEnum": [
        "all",
    ],
    "DefaultPayeePolicyEnum": [
        "deny",
    ],
    "SequencerUpdateEnum": [
        "allow_all",
    ],
    "ProverIncentivesEnum": [
        "exit",
    ],
    "RevenueShareEnum": [
        "activate_revenue_share",
        "deactivate_revenue_share",
    ],
    "AdminEnum": [
        "None",
    ],
    "IsmEnum": [
        "AlwaysTrust",
    ],
    "TokenSourceEnum": [
        "Native",
    ],
};
