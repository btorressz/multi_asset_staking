use std::convert::TryInto;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, TokenAccount, Transfer, Token};
use pyth_sdk_solana::load_price_feed_from_account_info;
use pyth_sdk::PriceFeed;

declare_id!("AEHQZPF6NUTXjtntjUXfw7xEx8CYi9XKb4t2aXjN1n9w");

const REWARD_INTERVAL: i64 = 86400; // Set to 24 hours (in seconds)

#[program]
mod multi_asset_staking {
    use super::*;

    // Initialize user portfolio with initial assets
    pub fn initialize_portfolio(ctx: Context<InitializePortfolio>, initial_assets: Vec<u64>) -> Result<()> {
        let portfolio = &mut ctx.accounts.portfolio;
        portfolio.owner = *ctx.accounts.user.key;
        portfolio.assets = initial_assets;
        portfolio.value_history.push(0); // Start with zero value history
        portfolio.returns = 0;
        Ok(())
    }

    // Stake assets into the protocol
    pub fn stake_assets(ctx: Context<StakeAssets>, amounts: Vec<u64>) -> Result<()> {
        let portfolio = &mut ctx.accounts.portfolio;

        // Ensure user is staking appropriate amounts
        require!(amounts.len() == portfolio.assets.len(), StakingError::InvalidAmount);
        for (i, &amount) in amounts.iter().enumerate() {
            portfolio.assets[i] += amount;
        }

        // Transfer assets from user to staking vault (Assume vault already set up)
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.vault_token_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(CpiContext::new(cpi_program, cpi_accounts), amounts.iter().sum())?;

        Ok(())
    }

    // Withdraw staked assets
    pub fn withdraw_assets(ctx: Context<WithdrawAssets>, amounts: Vec<u64>) -> Result<()> {
        let portfolio = &mut ctx.accounts.portfolio;

        // Ensure the user has enough staked assets to withdraw
        require!(amounts.len() == portfolio.assets.len(), StakingError::InvalidAmount);
        for (i, &amount) in amounts.iter().enumerate() {
            require!(portfolio.assets[i] >= amount, StakingError::InsufficientFunds);
            portfolio.assets[i] -= amount;
        }

        // Transfer assets back to user
        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_token_account.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(CpiContext::new(cpi_program, cpi_accounts), amounts.iter().sum())?;

        Ok(())
    }

    // Rebalance the portfolio with new asset allocations
    pub fn rebalance(ctx: Context<Rebalance>, new_allocation: Vec<u64>) -> Result<()> {
        let portfolio = &mut ctx.accounts.portfolio;

        // Rebalance logic: Ensure the new allocation matches the number of assets
        require!(new_allocation.len() == portfolio.assets.len(), StakingError::InvalidRebalance);
        portfolio.assets = new_allocation;

        Ok(())
    }

    // Rebalance the portfolio with risk management
    pub fn rebalance_with_risk_management(ctx: Context<Rebalance>, new_allocation: Vec<u64>, max_allocation: u64) -> Result<()> {
        let portfolio = &mut ctx.accounts.portfolio;
        for &allocation in new_allocation.iter() {
            require!(allocation <= max_allocation, StakingError::ExceedsMaxAllocation);
        }
        portfolio.assets = new_allocation;
        Ok(())
    }

    // Update portfolio performance based on current value
    pub fn update_performance(ctx: Context<UpdatePerformance>, current_value: u64) -> Result<()> {
        let portfolio = &mut ctx.accounts.portfolio;
        let previous_value = portfolio.value_history.last().unwrap_or(&0);
        let returns = current_value.saturating_sub(*previous_value);
        portfolio.returns += returns;
        portfolio.value_history.push(current_value);
        Ok(())
    }

    // Distribute rewards to the portfolio based on returns
    pub fn distribute_rewards(ctx: Context<DistributeRewards>, reward_amount: u64) -> Result<()> {
        let portfolio = &mut ctx.accounts.portfolio;

        // Transfer rewards from reward pool to user
        let cpi_accounts = Transfer {
            from: ctx.accounts.reward_pool.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.reward_authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(CpiContext::new(cpi_program, cpi_accounts), reward_amount)?;

        Ok(())
    }

    // Automate reward distribution based on time intervals
    pub fn auto_distribute_rewards(ctx: Context<AutoDistributeRewards>, reward_rate: u64) -> Result<()> {
        let clock = Clock::get()?;
        let last_reward_time = ctx.accounts.reward_pool.last_reward_time;
        let current_time = clock.unix_timestamp;

        // Distribute rewards only if sufficient time has passed
        require!(current_time - last_reward_time >= REWARD_INTERVAL, StakingError::RewardsNotYetAvailable);

        let reward_amount = reward_rate * ((current_time - last_reward_time) as u64);

        let cpi_accounts = Transfer {
            from: ctx.accounts.reward_pool.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.reward_authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(CpiContext::new(cpi_program, cpi_accounts), reward_amount)?;

        ctx.accounts.reward_pool.last_reward_time = current_time;
        Ok(())
    }

    // Emergency unstake with penalty
    pub fn emergency_unstake(ctx: Context<EmergencyUnstake>, penalty: u64) -> Result<()> {
        let portfolio = &mut ctx.accounts.portfolio;
        
        // Apply a penalty to the staked amount
        for asset in portfolio.assets.iter_mut() {
            *asset = asset.saturating_sub(penalty); // Deduct penalty from all assets
        }

        // Clear assets for emergency withdrawal
        portfolio.assets.clear();
        
        Ok(())
    }

    // Create a governance proposal
    pub fn create_proposal(ctx: Context<CreateProposal>, description: String) -> Result<()> {
        let governance = &mut ctx.accounts.governance;
        governance.proposal = description;
        governance.votes_for = 0;
        governance.votes_against = 0;
        Ok(())
    }

    // Cast a vote on a governance proposal
    pub fn cast_vote(ctx: Context<CastVote>, vote: bool) -> Result<()> {
        let governance = &mut ctx.accounts.governance;
        if vote {
            governance.votes_for += 1;
        } else {
            governance.votes_against += 1;
        }
        Ok(())
    }

    // Stake governance tokens for voting power
    pub fn stake_governance_tokens(ctx: Context<StakeGovernanceTokens>, amount: u64) -> Result<()> {
        let governance_stake = &mut ctx.accounts.governance_stake;
        governance_stake.staked_amount += amount;

        let cpi_accounts = Transfer {
            from: ctx.accounts.user_governance_token_account.to_account_info(),
            to: ctx.accounts.vault_governance_token_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(CpiContext::new(cpi_program, cpi_accounts), amount)?;

        Ok(())
    }

pub fn update_portfolio_value_with_oracle(ctx: Context<UpdateWithOracle>, _price_account: Pubkey) -> Result<()> {
    let price_feed: PriceFeed = load_price_feed_from_account_info(&ctx.accounts.oracle_account)
        .map_err(|_| StakingError::OracleLoadError)?;  // Convert PythError to custom Anchor error

    let portfolio = &mut ctx.accounts.portfolio;
    let mut total_value = 0;

    // Define a maximum age for the price (e.g., 60 seconds) as `u64`
    let max_age: u64 = 60 * 1_000_000_000;  // 60 seconds in nanoseconds
    let current_timestamp = Clock::get()?.unix_timestamp;

    // Keep `current_timestamp` as `i64` and `max_age` as `u64`
    let current_timestamp_i64: i64 = current_timestamp;  // No need for conversion

    // Safely retrieve the price within the acceptable time range
    let current_price_data = price_feed.get_price_no_older_than(current_timestamp_i64, max_age)
        .ok_or(StakingError::OracleLoadError)?;  // Handle the error if the price is too old or unavailable

    // Multiply each asset by the price
    for i in 0..portfolio.assets.len() {
        let asset_value = portfolio.assets[i] as u64 * current_price_data.price as u64;  // Access `price` field
        total_value += asset_value;
    }

    portfolio.value_history.push(total_value);
    Ok(())
}

    // Referral rewards for new users
    pub fn reward_referral(ctx: Context<RewardReferral>, referred_user: Pubkey, referral_reward: u64) -> Result<()> {
        let referral = &mut ctx.accounts.referral;
        referral.referred_user = referred_user;
        referral.reward += referral_reward;

        let cpi_accounts = Transfer {
            from: ctx.accounts.referral_pool.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.referral_authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(CpiContext::new(cpi_program, cpi_accounts), referral_reward)?;

        Ok(())
    }
}

// Define ReferralPool
#[account]
pub struct ReferralPool {
    pub total_rewards: u64,
}

// Define the new AutoDistributeRewards context
#[derive(Accounts)]
pub struct AutoDistributeRewards<'info> {
    #[account(mut)]
    pub reward_pool: Account<'info, RewardPool>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub reward_authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

// Define Portfolio account to track staked assets, performance, etc.
#[account]
pub struct Portfolio {
    pub owner: Pubkey,
    pub assets: Vec<u64>,            // Quantities of each staked asset
    pub value_history: Vec<u64>,     // Historical portfolio values
    pub returns: u64,                // Current portfolio returns
}

impl Portfolio {
    const LEN: usize = 32 + 8 * 10 + 8 * 10 + 8; // Assume up to 10 assets
}

// Define Governance account and its LEN constant
#[account]
pub struct Governance {
    pub proposal: String,
    pub votes_for: u64,
    pub votes_against: u64,
}

impl Governance {
    const LEN: usize = 8 + 4 + 8 + 8; // Adjust based on the actual fields
}

// Define GovernanceStake to track staked governance tokens
#[account]
pub struct GovernanceStake {
    pub staked_amount: u64,
    pub last_vote_timestamp: i64,
}

// Define RewardPool to hold tokens for reward distribution
#[account]
pub struct RewardPool {
    pub total_rewards: u64,
    pub last_reward_time: i64,
}

// Define Referral account
#[account]
pub struct Referral {
    pub referred_user: Pubkey,
    pub reward: u64,
}

// Define Contexts for each instruction

#[derive(Accounts)]
pub struct InitializePortfolio<'info> {
    #[account(init, payer = user, space = 8 + Portfolio::LEN)]
    pub portfolio: Account<'info, Portfolio>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct StakeAssets<'info> {
    #[account(mut)]
    pub portfolio: Account<'info, Portfolio>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct WithdrawAssets<'info> {
    #[account(mut)]
    pub portfolio: Account<'info, Portfolio>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Rebalance<'info> {
    #[account(mut, has_one = owner)]
    pub portfolio: Account<'info, Portfolio>,
    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct UpdatePerformance<'info> {
    #[account(mut, has_one = owner)]
    pub portfolio: Account<'info, Portfolio>,
    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct DistributeRewards<'info> {
    #[account(mut)]
    pub portfolio: Account<'info, Portfolio>,
    #[account(mut)]
    pub reward_pool: Account<'info, RewardPool>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub reward_authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct EmergencyUnstake<'info> {
    #[account(mut, has_one = owner)]
    pub portfolio: Account<'info, Portfolio>,
    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct CreateProposal<'info> {
    #[account(init, payer = proposer, space = 8 + Governance::LEN)]
    pub governance: Account<'info, Governance>,
    #[account(mut)]
    pub proposer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CastVote<'info> {
    #[account(mut)]
    pub governance: Account<'info, Governance>,
    pub voter: Signer<'info>,
}

#[derive(Accounts)]
pub struct StakeGovernanceTokens<'info> {
    #[account(mut)]
    pub governance_stake: Account<'info, GovernanceStake>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_governance_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_governance_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct UpdateWithOracle<'info> {
    #[account(mut)]
    pub portfolio: Account<'info, Portfolio>,
    #[account(mut)]
    pub oracle_account: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct RewardReferral<'info> {
    #[account(mut)]
    pub referral: Account<'info, Referral>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub referral_pool: Account<'info, ReferralPool>,
    #[account(mut)]
    pub referral_authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

// Define custom errors
#[error_code]
pub enum StakingError {
    #[msg("Invalid amount provided")]
    InvalidAmount,
    #[msg("Insufficient funds for withdrawal")]
    InsufficientFunds,
    #[msg("Unauthorized access to portfolio")]
    UnauthorizedAccess,
    #[msg("Invalid rebalance strategy")]
    InvalidRebalance,
    #[msg("Rewards not yet available")]
    RewardsNotYetAvailable,
    #[msg("Exceeds maximum allowed allocation")]
    ExceedsMaxAllocation,
    #[msg("Failed to load price feed from oracle")]
    OracleLoadError,
    #[msg("Invalid timestamp value")]
    InvalidTimestamp,  // Error for timestamp conversion failure
    #[msg("Invalid max age value")]
    InvalidMaxAge,  // Error for max age conversion failure
}
