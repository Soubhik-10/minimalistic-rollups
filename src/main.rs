use std::collections::{HashMap, VecDeque};

type Address = u64;
type Balance = u64;
type BlockNumber = u64;

#[derive(Clone, Debug)]
struct Transaction {
    from: Address,
    to: Address,
    amount: Balance,
}

#[derive(Clone, Debug)]
struct State {
    balances: HashMap<Address, Balance>,
}

impl State {
    fn new() -> Self {
        Self {
            balances: HashMap::new(),
        }
    }

    fn apply_tx(&mut self, tx: &Transaction) -> bool {
        let sender_balance = self.balances.get(&tx.from).copied().unwrap_or(0);
        if sender_balance >= tx.amount {
            *self.balances.entry(tx.from).or_default() -= tx.amount;
            *self.balances.entry(tx.to).or_default() += tx.amount;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
struct RollupBlock {
    block_number: BlockNumber,
    transactions: Vec<Transaction>,
    post_state: State,
    committed: bool,
}

#[derive(Debug, Clone)]
struct FraudChallenge {
    block_number: BlockNumber,
    tx_index: usize,
    challenger: Address,
    time: u64,
    valid: Option<bool>,
}

struct L1Verifier {
    time: u64,
    blocks: Vec<RollupBlock>,
    challenges: VecDeque<FraudChallenge>,
    resolved_challenges: Vec<FraudChallenge>,
    challenge_timeout: u64,
    initial_state: Option<State>, // 🔧 added to track pre-L2 state
}

impl L1Verifier {
    fn new(timeout: u64) -> Self {
        Self {
            time: 0,
            blocks: vec![],
            challenges: VecDeque::new(),
            resolved_challenges: vec![],
            challenge_timeout: timeout,
            initial_state: None, // 🔧
        }
    }

    fn submit_block(&mut self, block: RollupBlock) {
        println!("Block #{} submitted", block.block_number);
        if self.blocks.is_empty() {
            let mut initial_state = block.post_state.clone();
            for tx in block.transactions.iter().rev() {
                // Safe subtraction
                if let Some(to_balance) = initial_state.balances.get_mut(&tx.to) {
                    *to_balance = to_balance.saturating_sub(tx.amount);
                }
                *initial_state.balances.entry(tx.from).or_default() += tx.amount;
            }
            self.initial_state = Some(initial_state);
        }
        self.blocks.push(block);
    }

    fn submit_challenge(&mut self, challenge: FraudChallenge) {
        println!(
            "Fraud challenge submitted on block #{} tx[{}] by {}",
            challenge.block_number, challenge.tx_index, challenge.challenger
        );
        self.challenges.push_back(challenge);
    }

    fn advance_time(&mut self, ticks: u64) {
        self.time += ticks;
        println!("Advanced L1 time by {} ticks", ticks);
        self.process_challenges();
    }

    fn process_challenges(&mut self) {
        while let Some(mut challenge) = self.challenges.front().cloned() {
            if self.time - challenge.time >= self.challenge_timeout {
                let _ = self.challenges.pop_front();
                let block = &self.blocks[challenge.block_number as usize];
                let pre_state = self.reconstruct_state(challenge.block_number); // 🔧 uses initial_state
                let tx = &block.transactions[challenge.tx_index];

                let mut test_state = pre_state.clone();
                let expected_state = &block.post_state;
                let valid =
                    test_state.apply_tx(tx) && test_state.balances == expected_state.balances;

                challenge.valid = Some(valid);
                self.resolved_challenges.push(challenge.clone());

                if valid {
                    println!(
                        "Challenge resolved: ✅ VALID block at #{} tx[{}]",
                        challenge.block_number, challenge.tx_index
                    );
                } else {
                    println!(
                        "Challenge resolved: ❌ FRAUD detected at block #{} tx[{}]",
                        challenge.block_number, challenge.tx_index
                    );
                    self.blocks[challenge.block_number as usize].committed = false;
                }
            } else {
                break;
            }
        }
    }

    fn reconstruct_state(&self, upto_block: BlockNumber) -> State {
        let mut state = self.initial_state.clone().unwrap_or_else(State::new); // 🔧 use initial
        for b in 0..upto_block {
            for tx in &self.blocks[b as usize].transactions {
                state.apply_tx(tx);
            }
        }
        state
    }
}

fn main() {
    let mut l1 = L1Verifier::new(5); // timeout = 5 ticks

    let mut state = State::new();
    state.balances.insert(1, 100);
    state.balances.insert(2, 50);

    let tx1 = Transaction {
        from: 1,
        to: 2,
        amount: 40,
    };
    let tx2 = Transaction {
        from: 1,
        to: 2,
        amount: 1000,
    }; // Invalid transaction

    let mut block_state = state.clone();
    block_state.apply_tx(&tx1);
    block_state.apply_tx(&tx2); // Invalid tx still included in post_state

    let block = RollupBlock {
        block_number: 0,
        transactions: vec![tx1.clone(), tx2.clone()],
        post_state: block_state.clone(),
        committed: true,
    };

    l1.submit_block(block);

    let fraud_challenge = FraudChallenge {
        block_number: 0,
        tx_index: 1,
        challenger: 42,
        time: l1.time,
        valid: None,
    };

    l1.submit_challenge(fraud_challenge);
    l1.advance_time(6); // Exceeds timeout, triggers processing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_state() -> State {
        let mut state = State::new();
        state.balances.insert(1, 100);
        state.balances.insert(2, 50);
        state
    }

    #[test]
    fn test_valid_transaction_block() {
        let mut l1 = L1Verifier::new(5); // timeout = 5 ticks

        let mut state = State::new();
        state.balances.insert(1, 100);
        state.balances.insert(2, 50);

        // This will be used by both the block and L1 verifier
        let tx = Transaction {
            from: 1,
            to: 2,
            amount: 40,
        };

        // Simulate how L2 would compute post-state
        let mut post_state = state.clone();
        assert!(post_state.apply_tx(&tx)); // ensure tx is valid

        // Construct block from same state
        let block = RollupBlock {
            block_number: 0,
            transactions: vec![tx.clone()],
            post_state: post_state.clone(), // matches what L1 will compute
            committed: true,
        };

        l1.submit_block(block);

        // Submit challenge even though it's a valid tx
        let fraud_challenge = FraudChallenge {
            block_number: 0,
            tx_index: 0,
            challenger: 99,
            time: l1.time,
            valid: None,
        };

        l1.submit_challenge(fraud_challenge);
        l1.advance_time(6); // trigger fraud check

        // ✅ Should be resolved as valid
        assert_eq!(l1.resolved_challenges.len(), 1);
        assert_eq!(l1.resolved_challenges[0].valid, Some(true));
        assert!(l1.blocks[0].committed);
    }

    #[test]
    fn test_invalid_transaction_detected() {
        let mut l1 = L1Verifier::new(5);
        let state = setup_state();

        let tx = Transaction {
            from: 1,
            to: 2,
            amount: 1000,
        }; // Invalid tx
        let mut post_state = state.clone();
        post_state.apply_tx(&tx); // Still applies in mock rollup

        let block = RollupBlock {
            block_number: 0,
            transactions: vec![tx.clone()],
            post_state: post_state.clone(),
            committed: true,
        };

        l1.submit_block(block);

        let challenge = FraudChallenge {
            block_number: 0,
            tx_index: 0,
            challenger: 99,
            time: l1.time,
            valid: None,
        };

        l1.submit_challenge(challenge);
        l1.advance_time(6);

        assert_eq!(l1.resolved_challenges[0].valid, Some(false));
        assert!(!l1.blocks[0].committed);
    }

    #[test]
    fn test_challenge_before_timeout_not_processed() {
        let mut l1 = L1Verifier::new(10);
        let state = setup_state();

        let tx = Transaction {
            from: 1,
            to: 2,
            amount: 10,
        };
        let mut post_state = state.clone();
        post_state.apply_tx(&tx);

        let block = RollupBlock {
            block_number: 0,
            transactions: vec![tx.clone()],
            post_state: post_state.clone(),
            committed: true,
        };

        l1.submit_block(block);

        let challenge = FraudChallenge {
            block_number: 0,
            tx_index: 0,
            challenger: 77,
            time: l1.time,
            valid: None,
        };

        l1.submit_challenge(challenge);
        l1.advance_time(5); // not enough to trigger timeout

        assert!(l1.resolved_challenges.is_empty());
    }
}
