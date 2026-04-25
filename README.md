# CBOR-Web Token Engine (CBORW)

**Repo privé** — Ne pas partager. Le moteur économique de CBOR-Web.

[![Rust](https://img.shields.io/badge/Rust-1.93-000000?logo=rust)](https://rust-lang.org)
[![Solidity](https://img.shields.io/badge/Solidity-0.8-363636?logo=solidity)](https://soliditylang.org)
[![Status](https://img.shields.io/badge/status-prototype%20ready-orange)]()

---

## Pourquoi ce repo existe

CBOR-Web a besoin d'un **jeton utilitaire** pour son modèle économique. Les agents IA doivent détenir ≥ 1 CBORW pour accéder au contenu premium (L1) des sites CBOR-Web.

Plutôt que de lancer directement un token sur Ethereum (frais de gas, audit coûteux, dépendance externe),
j'ai conçu une **stratégie en deux phases**.

---

## Stratégie deux phases

### Phase 1 — Token autonome (maintenant)
| Serveur Rust | Proto maison |
|--------------|-------------|

- **Où** : MiniPC ou MacPro (infra ExploDev existante)
- **Coût** : 0 €
- **Délai** : Fonctionne aujourd'hui
- **Fonctionnalités** : Ledger, transferts, vérification d'accès, persistance JSON

**Pourquoi c'est malin :**
- Permet de **tester le modèle économique en conditions réelles** sans risquer d'argent
- Le cbor-server peut vérifier les soldes en temps réel via HTTP
- Aucune dépendance externe (pas de blockchain, pas de gas, pas de MetaMask)
- Si le protocole CBOR-Web décolle, on migre en Phase 2
- Si ça ne décolle pas, on n'a rien perdu

### Phase 2 — ERC-20 sur Ethereum (quand l'adoption est là)

| Contrat Solidity | Déploiement Sepolia | Puis mainnet |
|------------------|---------------------|--------------|

- **Déclencheur** : ≥ 200 sites CBOR-Web actifs ET ≥ 50 agents utilisateurs
- **Coût estimé** : 5 000 - 20 000 € (audit Certik/OpenZeppelin + gas mainnet)
- Le contrat `CborWebToken.sol` est **déjà écrit**, audité en interne, prêt à déployer
- La migration du proto vers ERC-20 est prévue : les adresses et soldes seront transférés

---

## Architecture Phase 1

```
┌──────────────────────────────────────────────┐
│                  Agent IA                     │
│  (ChatGPT, Claude, crawler, RAG pipeline)     │
│  Possède une adresse wallet (ex: 0xABC...)   │
└──────────────────┬───────────────────────────┘
                   │ GET /verify-access?address=0xABC...
                   ▼
┌──────────────────────────────────────────────┐
│              cbor-token (Rust)                │
│  Port 3002 — MiniPC / MacPro                 │
│                                              │
│  ┌────────────┐   ┌────────────────────┐     │
│  │   Ledger   │   │  Signature verify   │     │
│  │  (JSON)    │   │  (ecrecover)        │     │
│  └────────────┘   └────────────────────┘     │
│                                              │
│  API REST :                                   │
│  • /balance?address=        → solde          │
│  • /verify-access?address=  → accès OK/KO    │
│  • /transfer (POST)         → envoi tokens   │
│  • /total-supply            → offre totale    │
│  • /holders                 → top détenteurs │
│  • /stats                   → statistiques   │
└──────────────────┬───────────────────────────┘
                   │ has_access: true/false
                   ▼
┌──────────────────────────────────────────────┐
│              cbor-server (Rust)               │
│  Port 3001                                    │
│                                               │
│  Avant de servir une page CBOR :              │
│  1. Lit X-CBOR-Web-Wallet du header HTTP      │
│  2. Appelle /verify-access du token server    │
│  3. Si has_access=true → contenu complet      │
│  4. Si false → contenu L0 (public) seulement  │
└──────────────────────────────────────────────┘
```

---

## Installation Phase 1

### Prérequis
- Rust 1.70+
- Une machine Linux (MiniPC ou MacPro recommandé, Pi5 trop faible)
- Port 3002 ouvert

### Build
```bash
git clone https://github.com/ploteddie-bit/cbor-token.git
cd cbor-token
cargo build --release
```

### Lancement
```bash
./target/release/cbor-token \
  --listen 0.0.0.0:3002 \
  --ledger-file /srv/cbor-token/ledger.json \
  --founder "0xTON_ADRESSE_ETHEREUM" \
  --total-supply 100000000 \
  --min-hold 1
```

**Mode prototype (sans vérification de signature)** :
```bash
./target/release/cbor-token \
  --listen 0.0.0.0:3002 \
  --ledger-file /srv/cbor-token/ledger.json \
  --founder "0xADRESSE_FONDATEUR" \
  --total-supply 100000000
# --verify-signatures est false par défaut
```

**Mode production (avec ecrecover)** :
```bash
./target/release/cbor-token \
  --listen 0.0.0.0:3002 \
  --ledger-file /srv/cbor-token/ledger.json \
  --founder "0xADRESSE_ETH" \
  --total-supply 100000000 \
  --verify-signatures true
```

### Service systemd
```ini
[Unit]
Description=CBOR-Web Token Server
After=network.target

[Service]
ExecStart=/opt/cbor-token/target/release/cbor-token \
  --listen 0.0.0.0:3002 \
  --ledger-file /srv/cbor-token/ledger.json \
  --founder "0xADRESSE" \
  --total-supply 100000000
Restart=always
User=cborweb

[Install]
WantedBy=multi-user.target
```

---

## API Reference

### `GET /total-supply`
```json
{"total_supply": 100000000, "symbol": "CBORW", "decimals": 0, "min_hold_for_access": 1}
```

### `GET /balance?address=0xABC...`
```json
{"address": "0xABC...", "balance": 500, "has_access": true}
```

### `GET /verify-access?address=0xABC...&sig=...&msg=...`
Appelé par cbor-server pour décider si un agent peut lire du contenu L1.
```json
{"address": "0xABC...", "balance": 500, "has_access": true, "min_hold": 1}
```

### `POST /transfer`
```json
// Requête
{"from": "0xABC...", "to": "0xDEF...", "amount": 100, "sig": "0x..."}

// Réponse
{"success": true, "from_balance": 400, "to_balance": 600}
```

### `GET /holders`
Top 100 détenteurs par solde.

### `GET /stats`
```json
{"total_supply": 100000000, "circulating": 50000000, "holders": 3, "symbol": "CBORW"}
```

---

## Tokenomics

| Paramètre | Valeur |
|-----------|--------|
| Supply totale | 100 000 000 CBORW |
| Décimales | 0 (jeton entier, comme les actions) |
| Accès minimum | 1 CBORW |
| Allocation fondateur | 18 000 000 (18%) — vesting 12 mois |
| Allocation écosystème | 40 000 000 (40%) |
| Allocation communauté | 20 000 000 (20%) |
| Allocation développement | 10 000 000 (10%) |
| Allocation liquidité | 8 000 000 (8%) |
| Allocation advisors | 4 000 000 (4%) |

**Modèle hold-to-access** : l'agent **détient** le token, il ne le **dépense** pas. Cela réduit la pression de vente.

---

## Migration Phase 1 → Phase 2

Quand le seuil de 200 sites + 50 agents est atteint :

1. Déployer `contracts/CborWebToken.sol` sur testnet Sepolia
2. Auditer le contrat (Certik ou OpenZeppelin)
3. Migrer les soldes du ledger JSON vers le contrat
4. Déployer sur Ethereum mainnet
5. Mettre à jour cbor-server pour appeler le contrat au lieu du token server
6. Lister sur Uniswap (DEX) pour la liquidité publique

Le contrat Solidity est déjà compatible OpenZeppelin v5.x et inclut :
- `verifyAccess(address)` → vérification on-chain
- `verifyAccessBatch(address[])` → vérification par lot
- `unlockFounder()` → déblocage après 12 mois de cliff

---

## Sécurité

- **Phase 1** : Ledger local, signature ecrecover optionnelle, pas de surface d'attaque externe
- **Phase 2** : Contrat standard ERC-20 audité, pas de fonctions custom risquées, ownership verrouillé

---

## Auteur

Eddie Plot — ExploDev / Deltopide SL — Avril 2026

*"Le meilleur moment pour planter un arbre c'était il y a 20 ans. Le deuxième meilleur moment c'est maintenant."*
