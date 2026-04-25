# Déploiement local CBORW (Anvil)

## Lancement
```bash
# Terminal 1 : lancer Anvil
anvil -p 8551

# Terminal 2 : déployer
cd deploy-local
forge create src/CborWebToken.sol:CborWebToken \
  --rpc-url http://127.0.0.1:8551 \
  --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

## Vérifier
```bash
CONTRACT=0x...  # adresse retournée par forge create
OWNER=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266

# Vérifier l'accès
cast call $CONTRACT "verifyAccess(address)(bool,uint256)" $OWNER --rpc-url http://127.0.0.1:8551
# → true, 100000000000000000000000000

# Vérifier un inconnu
cast call $CONTRACT "verifyAccess(address)(bool,uint256)" 0x0000000000000000000000000000000000000001 --rpc-url http://127.0.0.1:8551
# → false, 0
```
