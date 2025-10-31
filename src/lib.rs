#[cfg(test)]
mod tests {
    use alloy::{
        providers::{Provider, ProviderBuilder, ext::AnvilApi},
        rpc::types::{
            Filter,
            anvil::{ReorgOptions, TransactionData},
        },
        sol,
    };

    use alloy_node_bindings::Anvil;

    sol! {
        // Built directly with solc 0.8.30+commit.73712a01.Darwin.appleclang
        #[sol(rpc, bytecode="608080604052346015576101b0908161001a8239f35b5f80fdfe6080806040526004361015610012575f80fd5b5f3560e01c90816306661abd1461016157508063a87d942c14610145578063d732d955146100ad5763e8927fbc14610048575f80fd5b346100a9575f3660031901126100a9575f5460018101809111610095576020817f7ca2ca9527391044455246730762df008a6b47bbdb5d37a890ef78394535c040925f55604051908152a1005b634e487b7160e01b5f52601160045260245ffd5b5f80fd5b346100a9575f3660031901126100a9575f548015610100575f198101908111610095576020817f53a71f16f53e57416424d0d18ccbd98504d42a6f98fe47b09772d8f357c620ce925f55604051908152a1005b60405162461bcd60e51b815260206004820152601860248201527f436f756e742063616e6e6f74206265206e6567617469766500000000000000006044820152606490fd5b346100a9575f3660031901126100a95760205f54604051908152f35b346100a9575f3660031901126100a9576020905f548152f3fea2646970667358221220471585b420a1ad0093820ff10129ec863f6df4bec186546249391fbc3cdbaa7c64736f6c634300081e0033")]
        contract TestCounter {
            uint256 public count;

            #[derive(Debug)]
            event CountIncreased(uint256 newCount);
            #[derive(Debug)]
            event CountDecreased(uint256 newCount);

            function increase() public {
                count += 1;
                emit CountIncreased(count);
            }

            function decrease() public {
                require(count > 0, "Count cannot be negative");
                count -= 1;
                emit CountDecreased(count);
            }

            function getCount() public view returns (uint256) {
                return count;
            }
        }
    }

    #[tokio::test]
    async fn test_reorg_all_log_emissions() -> anyhow::Result<()> {
        // configure Anvil to mint a block per each transaction
        let anvil = Anvil::new().try_spawn()?;
        let provider = ProviderBuilder::new()
            .wallet(anvil.wallet().unwrap())
            .connect(anvil.endpoint().as_str())
            .await?;

        let contract = TestCounter::deploy(provider.clone()).await?;

        // initial event emissions
        let event_count = 5;
        for _ in 0..event_count {
            let _ = contract.increase().send().await?.get_receipt().await?;
        }

        let contract_deployment_block = provider
            .get_block(1.into())
            .await?
            .expect("block should exist");
        assert_eq!(1, contract_deployment_block.transactions.len());

        let last_event_block = contract_deployment_block.number() + event_count;

        // assert that each block has exactly 1 transaction
        for i in 2..=last_event_block {
            let block = provider
                .get_block(i.into())
                .await?
                .expect("block should exist");
            assert_eq!(1, block.transactions.len());
        }

        // assert the total number of logs in the whole chain
        let log_filter = &Filter::new().from_block(0).to_block(last_event_block);

        let logs = provider.get_logs(log_filter).await?;
        assert_eq!(event_count, logs.len() as u64);

        // reorg the last 5 blocks, re-emitting only 3 events
        let reorg_depth = event_count;
        let new_event_count = 3;

        let tx_block_pairs = (0..new_event_count)
            .map(|_| {
                let tx = contract.increase().into_transaction_request();
                (TransactionData::JSON(tx), 0)
            })
            .collect();

        let opts = ReorgOptions {
            depth: reorg_depth,
            tx_block_pairs,
        };
        provider.anvil_reorg(opts).await?;

        // after the reorg, the block when contract was deployed should be unchanged
        let post_reorg_contract_deployment_block = provider
            .get_block(contract_deployment_block.number().into())
            .await?
            .expect("block should exist");
        assert_eq!(
            contract_deployment_block.hash(),
            post_reorg_contract_deployment_block.hash()
        );
        assert_eq!(1, post_reorg_contract_deployment_block.transactions.len());

        // the next block should contain all of the new transactions
        let txs_block = provider
            .get_block(2.into())
            .await?
            .expect("block should exist");
        assert_eq!(new_event_count, txs_block.transactions.len());

        // other blocks should contain no transactions
        for i in 3..last_event_block {
            let block = provider
                .get_block(i.into())
                .await?
                .expect("block should exist");
            assert!(block.transactions.is_empty());
        }

        // reassert the number of logs in the whole chain
        let logs = provider.get_logs(log_filter).await?;
        assert_eq!(new_event_count, logs.len()); // FAIL: logs.len() somehow equals 0 (zero)

        Ok(())
    }

    #[tokio::test]
    async fn test_reorg_all_log_emissions_but_first() -> anyhow::Result<()> {
        // configure Anvil to mint a block per each transaction
        let anvil = Anvil::new().try_spawn()?;
        let provider = ProviderBuilder::new()
            .wallet(anvil.wallet().unwrap())
            .connect(anvil.endpoint().as_str())
            .await?;

        let contract = TestCounter::deploy(provider.clone()).await?;

        // initial event emissions
        let event_count = 5;
        for _ in 0..event_count {
            let _ = contract.increase().send().await?.get_receipt().await?;
        }

        let contract_deployment_block = provider
            .get_block(1.into())
            .await?
            .expect("block should exist");
        assert_eq!(1, contract_deployment_block.transactions.len());

        let last_event_block = contract_deployment_block.number() + event_count;

        // assert that each block has exactly 1 transaction
        for i in 2..=last_event_block {
            let block = provider
                .get_block(i.into())
                .await?
                .expect("block should exist");
            assert_eq!(1, block.transactions.len());
        }

        // assert the total number of logs in the whole chain
        let log_filter = &Filter::new().from_block(0);

        let logs = provider.get_logs(log_filter).await?;
        assert_eq!(event_count, logs.len() as u64);

        // reorg the last 4 blocks, re-emitting only 3 events
        let reorg_depth = event_count - 1;
        let new_event_count = 3;

        let tx_block_pairs = (0..new_event_count)
            .map(|_| {
                let tx = contract.increase().into_transaction_request();
                (TransactionData::JSON(tx), 0)
            })
            .collect();

        let opts = ReorgOptions {
            depth: reorg_depth,
            tx_block_pairs,
        };
        provider.anvil_reorg(opts).await?;

        // after the reorg, the block when contract was deployed should be unchanged
        let post_reorg_contract_deployment_block = provider
            .get_block(contract_deployment_block.number().into())
            .await?
            .expect("block should exist");
        assert_eq!(
            contract_deployment_block.hash(),
            post_reorg_contract_deployment_block.hash()
        );
        assert_eq!(1, post_reorg_contract_deployment_block.transactions.len());

        // after the reorg, the block with the first log emission should be unchanged
        let txs_block = provider
            .get_block(2.into())
            .await?
            .expect("block should exist");
        assert_eq!(1, txs_block.transactions.len());

        // the next block should contain all of the new transactions
        let txs_block = provider
            .get_block(3.into())
            .await?
            .expect("block should exist");
        assert_eq!(new_event_count, txs_block.transactions.len());

        // other blocks should contain no transactions
        for i in 4..last_event_block {
            let block = provider
                .get_block(i.into())
                .await?
                .expect("block should exist");
            assert!(block.transactions.is_empty());
        }

        // reassert the number of logs in the whole chain is:
        // all the newly emitted logs + the 1 old log
        let logs = provider.get_logs(log_filter).await?;
        assert_eq!(new_event_count + 1, logs.len());

        Ok(())
    }
}
