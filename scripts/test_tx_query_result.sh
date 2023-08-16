NEUTROND_BIN=neutrond
GAIAD_BIN=gaiad

CONTRACT=./artifacts/neutron_interchain_queries.wasm

CHAIN_ID_1=test-1
CHAIN_ID_2=test-2

NEUTRON_DIR=${NEUTRON_DIR:-../neutron}
HOME_1=${NEUTRON_DIR}/data/test-1/
HOME_2=${NEUTRON_DIR}/data/test-2/

FAUCET=demowallet3
USERNAME_2=demowallet2
KEY_2=$(gaiad keys show demowallet2 -a --keyring-backend test --home ${HOME_2})
ADMIN=$(neutrond keys show demowallet1 -a --keyring-backend test --home ${HOME_1})

TARGET_ADDRESS=cosmos17dtl0mjt3t77kpuhg2edqzjpszulwhgzuj9ljs
VAL2=cosmosvaloper1qnk2n4nlkpw9xfqntladh74w6ujtulwnmxnh3k
TEST_WALLET=test_wallet

yes | ${NEUTROND_BIN} keys add ${TEST_WALLET} --home ${HOME_1} --keyring-backend=test
TEST_ADDR=$(${NEUTROND_BIN} keys show ${TEST_WALLET} --keyring-backend test -a --home ${HOME_1})
${NEUTROND_BIN} tx bank send ${FAUCET} ${TEST_ADDR} 100000000stake --chain-id ${CHAIN_ID_1} --home ${HOME_1} --node tcp://localhost:16657 --keyring-backend test -y --gas-prices 0.0025stake --broadcast-mode=block


# Upload the queries contract
RES=$(${NEUTROND_BIN} tx wasm store ${CONTRACT} --from ${TEST_ADDR} --gas 50000000  --chain-id ${CHAIN_ID_1} --broadcast-mode=block --gas-prices 0.0025stake  -y --output json  --keyring-backend test --home ${HOME_1} --node tcp://127.0.0.1:16657)
QUERIES_CONTRACT_CODE_ID=$(echo $RES | jq -r '.logs[0].events[1].attributes[0].value')
echo $RES
echo $QUERIES_CONTRACT_CODE_ID

# Instantiate the queries contract
INIT_QUERIES_CONTRACT='{}'

RES=$(${NEUTROND_BIN} tx wasm instantiate $QUERIES_CONTRACT_CODE_ID "$INIT_QUERIES_CONTRACT" --from ${TEST_ADDR} --admin ${ADMIN} -y --chain-id ${CHAIN_ID_1} --output json --broadcast-mode=block --label "init"  --keyring-backend test --gas-prices 0.0025stake --home ${HOME_1} --node tcp://127.0.0.1:16657)
echo $RES
QUERIES_CONTRACT_ADDRESS=$(echo $RES | jq -r '.logs[0].events[0].attributes[0].value')
echo $QUERIES_CONTRACT_ADDRESS

# Send some money to contract for deposit
RES=$(${NEUTROND_BIN} tx bank send ${TEST_ADDR} ${QUERIES_CONTRACT_ADDRESS} 1000000stake --chain-id ${CHAIN_ID_1}  --broadcast-mode=block --gas-prices 0.0025stake -y --output json --keyring-backend test --home ${HOME_1} --node tcp://127.0.0.1:16657)
echo $RES

# Register a query for Send transactions
RES=$(${NEUTROND_BIN} tx wasm execute $QUERIES_CONTRACT_ADDRESS "{\"register_transfers_query\": {\"connection_id\": \"connection-0\", \"recipient\": \"${TARGET_ADDRESS}\", \"update_period\": 5, \"min_height\": 1}}" --from ${TEST_ADDR}  -y --chain-id ${CHAIN_ID_1} --output json --broadcast-mode=block --gas-prices 0.0025stake --gas 1000000 --keyring-backend test --home ${HOME_1} --node tcp://127.0.0.1:16657)
echo $RES

# Issue a Send transaction that we will be querying for
for i in `seq 0 100`; do
RES=$(${GAIAD_BIN} tx bank send ${KEY_2} ${TARGET_ADDRESS} 1000stake --sequence ${i} --from ${USERNAME_2} --gas 50000000 --gas-adjustment 1.4 --gas-prices 0.5stake --broadcast-mode sync --chain-id ${CHAIN_ID_2} --keyring-backend test --home ${HOME_2} --node tcp://127.0.0.1:26657 -y)
echo $RES
done
