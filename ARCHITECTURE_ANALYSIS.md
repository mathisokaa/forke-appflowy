# AppFlowy Cloud — Analyse d'architecture (rétro-ingénierie)

> Analyse du dépôt open-source **[AppFlowy-IO/AppFlowy-Cloud](https://github.com/AppFlowy-IO/AppFlowy-Cloud)** (licence **AGPLv3**), commit `581f123`, vendored dans ce fork le 2026-08-18 à des fins d'analyse. Aucune ligne de logique métier n'a été modifiée pour produire ce document (seul un fixture de test contenant un faux token OAuth a été neutralisé pour satisfaire le secret-scanning de GitHub, voir `tests/user/sign_in.rs:89`).
>
> Toutes les références `fichier:ligne` pointent vers les fichiers tels que vendored à la racine de ce dépôt.

## Table des matières

1. [Cartographie générale](#1-cartographie-générale)
2. [API et routes](#2-api-et-routes)
3. [Authentification](#3-authentification)
4. [Modèle de données](#4-modèle-de-données)
5. [Synchronisation temps réel (CRDT)](#5-synchronisation-temps-réel-crdt)
6. [⚠️ Application des limites de plan](#6-️-application-des-limites-de-plan)
7. [Intégration IA](#7-intégration-ia)
8. [Déploiement](#8-déploiement)
9. [Résumé exécutif](#9-résumé-exécutif)

---

## 1. Cartographie générale

### 1.0 Deux offres, un seul dépôt open-source

Point préalable indispensable pour comprendre tout le reste du document. Le `README.md` du dépôt est explicite (`README.md:20-51`) :

- **AppFlowy Managed Cloud** et **AppFlowy Self-hosted Cloud** (l'offre payante avec paliers Free/Pro/Team/AiMax/AiLocal) tournent sur un **fork fermé** : *"The codebase behind these two setups is a closed-source fork of this open-source codebase [...] combined with our proprietary code"* (`README.md:28`). Ce fork fermé est distribué sous une licence commerciale séparée, `AppFlowy-IO/AppFlowy-SelfHost-Commercial` (`README.md:29`), dont le fichier `SELF_HOST_LICENSE_AGREEMENT.md` (présent dans **ce** dépôt à titre de référence, mais qui **ne s'applique pas à ce dépôt**) décrit un modèle à clé de licence par serveur, non-copie/non-modification, etc.
- **Ce dépôt** (`AppFlowy-IO/AppFlowy-Cloud`) est distribué sous **AGPLv3** (`LICENSE:1-2`, badge `README.md:10`) et la phrase *"You're free to use https://github.com/AppFlowy-IO/AppFlowy-Cloud governed by its license"* (`README.md:51`) confirme qu'il n'y a **aucune restriction contractuelle** au self-hosting de ce code précis.

Conséquence directe pour la section 6 : les limites de plan (1 siège, quotas IA, etc.) décrites sur appflowy.com concernent le fork commercial fermé, **pas** le code que nous analysons ici.

### 1.1 Crates du workspace (`Cargo.toml:175-207`)

Le workspace Rust racine (`Cargo.toml:1-4`, paquet `appflowy-cloud`) déclare 27 membres :

#### Binaires / services déployables

| Crate | Chemin | Rôle |
|---|---|---|
| `appflowy-cloud` (paquet racine) | `src/` | **Serveur HTTP + WebSocket monolithique** (framework actix-web). Point d'entrée `src/main.rs:1-25`. Assemble toutes les routes REST dans `src/application.rs:138-179`. Logique métier dans `src/biz/*`, handlers dans `src/api/*`. |
| `admin_frontend` | `admin_frontend/` | Panneau d'admin web (framework **Axum**, différent du serveur principal). Point d'entrée `admin_frontend/src/main.rs:1-40`. Gère signup/login/refresh/OAuth via GoTrue et sert les pages HTML de connexion (voir §3). Monté derrière nginx sous `/console`. |
| `appflowy-worker` | `services/appflowy-worker/` | Worker asynchrone séparé (import de données, embeddings, notifications email). Point d'entrée `services/appflowy-worker/src/main.rs:1-11`, écoute sur `[::]:4001`. Consomme des jobs Redis publiés par `appflowy_cloud`. |
| `xtask` | `xtask/` | Outil de dev local (`cargo run --package xtask`) pour orchestrer les process pendant les tests d'intégration ; **non déployé en production** (`xtask/src/main.rs:1-40`). |

#### Bibliothèques partagées (`libs/*`)

| Crate | Rôle |
|---|---|
| `services/appflowy-collaborate` | **Bibliothèque uniquement** (pas de `[[bin]]`), compilée dans `appflowy_cloud`. Contient le serveur de collaboration temps réel (WS v1/v2, groupes, persistance collab) — voir §5. |
| `libs/database` | Accès Postgres/SQLx : `access_request`, `chat`, `collab`, `file` (S3), `history`, `index`, `listener`, `notification`, `pg_row`, `publish`, `quick_note`, `resource_usage`, `template`, `user`, `workspace`. |
| `libs/database-entity` | DTOs partagés entre couche DB et API/clients. |
| `libs/client-api` | SDK client Rust HTTP/WS pour appeler l'API AppFlowy Cloud (utilisé par le client desktop / tests d'intégration, **pas** par le serveur). |
| `libs/client-api-entity`, `libs/client-api-test` | Agrégation de DTOs pour le client public / helpers de tests. |
| `libs/infra` | Utilitaires transverses : `env_util` (helpers `get_env_var`/`get_env_var_opt`, `libs/infra/src/env_util.rs:3-33`), fichiers, thread pool. |
| `libs/shared-entity` | DTOs de requête/réponse communs + wrapper `AppResponse`. Contient `dto/billing_dto.rs` (voir §6). |
| `libs/gotrue`, `libs/gotrue-entity` | Client HTTP GoTrue + DTOs/claims JWT (voir §3). |
| `libs/app-error` | Enum `AppError` central. |
| `libs/workspace-template` | Construction de la structure de dossiers par défaut d'un nouveau workspace. |
| `libs/access-control` | RBAC/ABAC via **Casbin** (`libs/access-control/src/casbin/*`), avec une implémentation `noops` quand le contrôle d'accès est désactivé. |
| `libs/collab-rt-entity`, `libs/collab-rt-protocol`, `libs/appflowy-proto` | Types du protocole de sync temps réel (wire format), couche locale au-dessus de la lib externe `collab`. |
| `libs/collab-stream` | Abstraction Redis Streams pour la diffusion des mises à jour collab (`stream_router.rs`, `awareness_gossip.rs`, `collab_update_sink.rs`, `lease.rs`). |
| `libs/client-websocket` | Client WebSocket cross-platform (natif/WASM). |
| `libs/appflowy-ai-client` | Client HTTP vers le service externe « AppFlowy AI » (voir §7). |
| `libs/tonic-proto` | Code généré gRPC (protobuf) pour le service d'historique. |
| `libs/mailer` | Envoi SMTP via `lettre`. |
| `libs/indexer` | Sous-système d'embeddings/recherche vectorielle (pgvector), config OpenAI/Azure. |
| `libs/llm-client` | Abstraction chat/completion OpenAI/Azure, utilisée pour résumer les résultats de recherche. |

### 1.2 Services externes requis

| Service | Rôle précis | Config / client (fichier:ligne) |
|---|---|---|
| **PostgreSQL** (avec extension `pgvector`) | Datastore principal : users, workspaces, objets collab (CRDT), snapshots, chat, embeddings. Migrations exécutées automatiquement au boot. | `APPFLOWY_DATABASE_URL` (défaut `postgres://postgres:password@localhost:5432/postgres`, `src/config/config.rs:200-203`) ; pool + migration : `src/application.rs:538-556`. |
| **Redis** | Bus pub/sub + streams pour la diffusion des mises à jour collab entre instances, store de session actix (admin_frontend et le serveur principal), cache du moteur d'accès Casbin. | `APPFLOWY_REDIS_URI` (défaut `redis://localhost:6379`, `src/config/config.rs:225`) ; `get_redis_client()` `src/application.rs:410-442`. |
| **GoTrue** (auth Supabase self-hosted) | Émission/validation des identités, JWT, OAuth (Google/GitHub/Discord/SAML), signup/magic-link. | `APPFLOWY_GOTRUE_BASE_URL`, `APPFLOWY_GOTRUE_JWT_SECRET` (défaut `hello456`), `APPFLOWY_GOTRUE_SERVICE_ROLE` — `src/config/config.rs:211-215` ; `get_gotrue_client()` `src/application.rs:558-566`. |
| **S3 / MinIO** | Stockage des fichiers/pièces jointes utilisateur, offload des gros blobs collab (> seuil), stockage exclusif des snapshots CRDT. | `S3Setting` `src/config/config.rs:57-66` ; client `get_aws_s3_client()` `src/application.rs:444-472`. |
| **SMTP (mailer)** | Emails transactionnels/notifications (optionnel — voir §6/§8). | `MailerSetting`, `src/config/config.rs:262-269` ; `get_mailer()` `src/application.rs:523-536`. |
| **AppFlowy AI** (service externe séparé, closed-source) | Proxy des requêtes de chat/complétion IA. | `AppFlowyAISetting` (host/port), `src/config/config.rs:76-89` ; client `libs/appflowy-ai-client`. Voir §7. |
| **OpenAI / Azure OpenAI** (optionnel) | Embeddings + résumés IA pour la recherche sémantique (indexer), pas pour le chat. | `libs/indexer/src/vector/embedder.rs:35-58`, clé `AI_OPENAI_API_KEY` / `AI_AZURE_OPENAI_*`. |
| **appflowy_search** (service externe séparé, closed-source, pas de crate correspondante dans ce workspace) | Indexation/recherche par mot-clé, appelé par `appflowy_cloud`. | `APPFLOWY_SEARCH_SERVICE_URL` (défaut `http://appflowy_search:4002`, `docker-compose.yml:151`). |
| **nginx** | Reverse-proxy unique devant tous les services. | `nginx/nginx.conf` (détails §8). |

---

## 2. API et routes

Le serveur principal (`src/`, actix-web) assemble ses scopes dans `src/application.rs:138-179`. `admin_frontend` (Axum) a son propre routeur (`admin_frontend/src/web_api.rs:34-65`, `admin_frontend/src/web_app.rs:22-64`).

### Auth (`admin_frontend`, base `/web-api`, backé par GoTrue)
- `POST /web-api/signin` → `sign_in_handler` (`admin_frontend/src/web_api.rs:36,352-380`)
- `GET /web-api/oauth-redirect` / `GET /web-api/oauth-redirect/token` → flux OAuth2 (`web_api.rs:37-38,382-443+`)
- `POST /web-api/signup` → `sign_up_handler` (`web_api.rs:39,521-553`)
- `POST /web-api/login-refresh/:refresh_token` → `login_refresh_handler` (`web_api.rs:40,324-348`)
- `POST /web-api/logout`, `/change-password`, `/oauth_login/:provider`, `/invite`, `/workspace/:id/invite`, `/workspace/:id/leave`, `/invite/:id/accept`, `/open_app`, `DELETE /delete-account` (`web_api.rs:41-51`)
- `POST/PUT/DELETE /web-api/admin/user*`, `/web-api/admin/sso*` (`web_api.rs:54-64`)
- Pages HTML : `admin_frontend/src/web_app.rs:29-42` (`/web/login`, `/web/login-callback`, `/web/payment-success`, `/web/home`, ...)
- Côté serveur principal, seul point d'entrée auth-adjacent : `GET /api/user/verify/{access_token}` → `verify_user_handler` (`src/api/user.rs:23,36-46`) → `verify_token` (`src/biz/user/user_verify.rs:20-74`), qui provisionne paresseusement l'utilisateur/workspace local au premier login.

### Workspace management (`src/api/workspace.rs`, base `/api/workspace`)
CRUD workspace, invitations, membres, settings, invite-codes : `workspace.rs:101-161,417-421`. Logique métier : `src/biz/workspace/{ops,invite,page_view,publish,quick_note,duplicate}.rs`.

### Collab / sync (`src/api/workspace.rs` + `src/api/ws.rs`)
- CRUD objets collab (`create/get/update/delete_collab_handler`, versions v1 legacy et v1 courant) : `workspace.rs:163-203,300-301,381-385`.
- WebSocket : `GET /ws/v1` → `establish_ws_connection_v1` (`src/api/ws.rs:31,66-106`) ; `GET /ws/v2/{workspace_id}` → `establish_ws_connection_v2` (`ws.rs:32,108-160`) ; legacy `GET /ws/{token}/{device_id}` (`ws.rs:40-64`).
- `POST /api/realtime/post/stream` → `post_realtime_message_stream_handler` (`workspace.rs:424-431`).

### IA (deux scopes)
- Complétion/embeddings — `src/api/ai.rs:22-33`, base `/api/ai/{workspace_id}` : `/complete/stream`, `/v2/complete/stream`, `/summarize_row`, `/translate_row`, `/local/config`, `/calculate_similarity`, `/model/list`.
- Chat — `src/api/chat.rs:39-105`, base `/api/chat/{workspace_id}` : création/suppression de chat, settings, messages question/réponse, streaming SSE, questions liées, contexte texte.

### Stockage / fichiers
- `src/api/file_storage.rs:42-87`, base `/api/file_storage` : blob PUT/GET/DELETE, métadonnées, usage, upload multipart (`create_upload`/`upload_part`/`complete_upload`), variantes `v1`.
- Avatars utilisateur : `src/api/user.rs:22-33` (`POST /api/user/asset/image`, `GET .../person/{id}/file/{id}`).
- Avatars template : `src/api/template.rs:57-58`.

### Billing/subscription
**Aucune route trouvée** (`GET /billing`, `/subscription`, `/payment` : zéro résultat dans `src/` et `admin_frontend/src`). Seule trace : une page statique de confirmation `GET /web/payment-success` (`admin_frontend/src/web_app.rs:34,74-76`) qui ne fait qu'afficher un template, sans traiter aucun paiement. Détails et implications en §6.

### Admin
Tout sous `admin_frontend`, `/web-api/admin/*` et `/web/admin/*` (`web_api.rs:54-64`, `web_app.rs:56-63`) : gestion utilisateurs/SSO via l'API admin de GoTrue. **Le serveur principal n'a aucune route `/api/admin/*`.**

### Search
`src/api/search.rs:17-22`, base `/api/search` : `GET /{workspace_id}`, `GET /{workspace_id}/summary`.

### Divers
- `GET /health` (`src/application.rs:169,568-570`), `GET /metrics` (`src/api/metrics.rs:17-25`), `GET /api/server` (`src/api/server_info.rs:8-11`).
- Profil utilisateur : `src/api/user.rs:21-33`.
- Import : `src/api/data_import.rs:31-38`.
- Template center : `src/api/template.rs:21-58`.
- Invite-code lookup : `src/api/invite_code.rs:14-16`.
- Partage invité : `src/api/guest.rs:23-37`.
- Demandes d'accès : `src/api/access_request.rs:25-32`.

---

## 3. Authentification

### 3.1 GoTrue comme fournisseur d'identité unique
Toute la gestion des identifiants est déléguée à une instance **GoTrue** (serveur d'auth de Supabase, self-hosted). Wrapper Rust : `libs/gotrue/src/api.rs` (`gotrue::api::Client`), instancié une fois au démarrage (`src/application.rs:558-566`).

Flux gérés par `admin_frontend` (surface de login principale) :
- **Signup** : `POST /web-api/signup` → `gotrue_client.sign_up` (`admin_frontend/src/web_api.rs:521-553`).
- **Login mot de passe** : `POST /web-api/signin` → `gotrue_client.token(Grant::Password)` (`web_api.rs:352-380`) ; mot de passe vide → repli sur magic link.
- **Magic link** : `gotrue_client.magic_link` (`libs/gotrue/src/api.rs:273-285`).
- **OAuth** : `GET /web-api/oauth-redirect` (validation `client_id`/`redirect_uri`) puis `GET /web-api/oauth-redirect/token` (échange du code) — `web_api.rs:382-443+`.
- **Refresh** : `POST /web-api/login-refresh/:refresh_token` → deux appels `gotrue_client.token(Grant::RefreshToken)` successifs (le 2e invalide le 1er) — `web_api.rs:324-348`.

Après obtention des tokens, `session_login(...)` stocke une `UserSession` dans Redis (`admin_frontend/src/session.rs:55-66`) et pose un cookie httpOnly `session_id` (`session.rs:297-301`). L'extracteur `UserSession` (`session.rs:131-171`) relit ce cookie à chaque requête admin_frontend et rafraîchit silencieusement le token GoTrue expiré (`session.rs:194-233`) — **sans vérifier la signature JWT localement** (commentaire explicite : *"no need to verify, let the appflowy cloud server do it"*, `session.rs:236`).

### 3.2 JWT — émission et secret
- Les JWT sont émis **entièrement par GoTrue**, jamais construits par le code Rust de ce dépôt pour les utilisateurs finaux.
- Claims : `GoTrueJWTClaims` (`libs/gotrue-entity/src/gotrue_jwt.rs:20-39`) — `sub`, `exp`, `email`, `role`, `app_metadata`, `session_id`, etc.
- Validation : `GoTrueJWTClaims::decode` (`gotrue_jwt.rs:61-65`) via `jsonwebtoken::decode`, algorithme **HS256 symétrique** — le backend et GoTrue doivent partager le même secret.
- Secret : `GoTrueSetting.jwt_secret` (`src/config/config.rs:69-73`), variable d'env **`APPFLOWY_GOTRUE_JWT_SECRET`, défaut `"hello456"`** (`src/config/config.rs:213`) — injecté comme `Data<Secret<String>>` partagé (`src/application.rs:175`).
- Le backend génère aussi son propre JWT de service (`role: service_role`, pas de `sub`) pour appeler l'API admin de GoTrue : `GoTrueAdmin::token()` (`src/state.rs:188-195`), avec le même secret partagé.

### 3.3 Vérification du token sur chaque requête
Pas de middleware `wrap()` global : la vérification se fait via des **extracteurs actix-web `FromRequest`** typés, dans `src/biz/authentication/jwt.rs` :
- `Authorization` (`jwt.rs:88-124`) et `UserUuid` (`jwt.rs:13-59`) — décodent le header `Authorization: Bearer <token>`, exigent le préfixe `Bearer `, appellent `authorization_from_token` → `GoTrueJWTClaims::decode` avec le secret partagé (`jwt.rs:126-163`, `gotrue_jwt.rs:62-65`). Échec = `401 Unauthorized`.
- `OptionalUserUuid` (`jwt.rs:63-86`) — même chose mais tolère l'absence d'auth (endpoints publics/anonymes).
- Chaque handler déclare simplement `user_uuid: UserUuid` en paramètre ; actix-web exécute l'extracteur avant le corps du handler — c'est l'équivalent fonctionnel d'un middleware, appliqué route par route via le système de types.
- WebSocket : le token est validé manuellement (arrive en query string), même fonction `authorization_from_token` appelée directement dans `establish_ws_connection_v1/v2` (`src/api/ws.rs:119,175`).
- `UserUuid` seul ne donne pas les permissions de workspace : les handlers appellent ensuite `state.workspace_access_control` / `state.collab_access_control` (RBAC Casbin, `libs/access-control`), ex. `src/api/workspace.rs:471-474,636-639`.

### 3.4 Pas de clé API séparée
Aucun mécanisme d'API-key côté client (`grep ApiKey/api_key/X-Api-Key` → rien). Tout client externe (desktop, mobile, web, intégration tierce) suit le même chemin : obtenir un JWT GoTrue puis le présenter en `Authorization: Bearer`. Le seul credential "serveur-à-serveur" distinct est le JWT `service_role` backend→GoTrue décrit en 3.2, qui n'est **jamais** accepté en substitut d'un JWT utilisateur sur les routes `/api/*` du backend (les claims de service n'ont pas de `sub`).

---

## 4. Modèle de données

63 fichiers de migration dans `migrations/`, de `20230312043024_user.sql` à `20250723072011_page_mention_notification_status.sql`.

### 4.1 Schéma cœur

- **`af_user`** (`migrations/20230312043024_user.sql:5-16`) — `uid BIGINT PK`, `uuid UUID` (lié à `auth.users` de GoTrue via FK ajoutée en `20231130150001_user_id_foreign_key.sql:11`), `email`, `name`, `metadata JSONB`.
- **RBAC** : `af_roles` (Owner/Member/Guest, `20230906101032_permission.sql:2-10`), `af_permissions` (4 niveaux d'accès, `:11-16`), `af_role_permissions` (table de jointure, `:36-40`).
- **`af_workspace`** (`20230906101223_workspace.sql:2-11`) — `workspace_id UUID PK`, `owner_uid` (FK `af_user`), **`workspace_type INTEGER NOT NULL DEFAULT 0`** (commentaire *"0: Free"*, `:7-8`) — voir §6. Colonnes ajoutées ensuite : `icon`, `settings JSONB` (`20240604090043_add_workspace_settings.sql:2`), `is_initialized`, `default_published_view_id`.
- **`af_workspace_member`** (`20230906101223_workspace.sql:14-21`) — PK composite `(uid, workspace_id)`, `role_id` (FK `af_roles`). Table de membership/rôle. Un trigger d'insertion crée automatiquement la ligne du owner (`20240123140707_workspace_owner_trigger.sql:1-15`).
- **`af_workspace_invitation`** / **`af_workspace_invite_code`** — invitations par email ou par code, avec trigger d'acceptation auto-insérant dans `af_workspace_member` (`20240304173938...`, `20250403021559...`).
- **`af_collab`** (objets CRDT) — créée partitionnée (`20230906102652_collab.sql:2-25`), puis **départitionnée** en une table plate `oid UUID PK` (`20250318120849_departition_af_collab.sql:6-19`). C'est la table canonique de stockage des documents/bases/dossiers CRDT.
- **`af_collab_snapshot`** (`20230906102652_collab.sql:60-69`) — snapshots ponctuels (`sid BIGSERIAL PK`, `blob BYTEA`).
- **`af_snapshot_meta`/`af_snapshot_state`** (`20240412083446_history_init.sql`) — système d'historique versionné séparé, chaînage de diffs.
- **`af_collab_embeddings`** (`20240614171931_collab_embeddings.sql:7-17`) — `embedding VECTOR(1536)` (pgvector), index sémantique du contenu collab.
- **`af_blob_metadata`** (`20230926145155_blob_storage.sql:1-8`) — métadonnées des fichiers uploadés (`file_size BIGINT` — c'est la colonne à partir de laquelle un usage de stockage *pourrait* être calculé, mais voir §6 : aucune agrégation/limite n'existe dans ce dépôt).
- **`af_published_collab`** + tables de commentaires/réactions — pages publiées publiquement.
- **`af_chat`** / **`af_chat_messages`** (`20240510024506_chat_message.sql`) — conversations IA.

Relations principales :
```
af_user 1--* af_workspace (owner_uid)
af_user 1--* af_workspace_member *--1 af_workspace   (rôle via af_roles)
af_workspace 1--* af_collab (documents/bases/dossiers CRDT)
af_collab 1--* af_collab_snapshot / af_snapshot_meta+state / af_collab_embeddings
af_workspace 1--* af_blob_metadata (fichiers)
af_workspace 1--* af_published_collab, af_chat, af_workspace_invitation
```

### 4.2 Tables/colonnes liées à un plan, quota ou limite (exhaustif)

**Constat central : aucune table de facturation/abonnement/quota n'existe dans `migrations/`.** Un grep exhaustif sur `subscription|billing|stripe|tier|quota|plan|ai_credit|ai_model|seat|member_limit|storage_limit|workspace_settings` sur tout le dossier `migrations/` ne renvoie **aucun** résultat. Ce qui existe :

| Élément | Fichier:ligne | Ce qu'il contrôle réellement | Défaut |
|---|---|---|---|
| `af_workspace.workspace_type` | `20230906101223_workspace.sql:7-8` | Palier de plan nominal — mais **seule la valeur 0 ("Free") est jamais utilisée** dans tout le code Rust ; jamais lu pour bloquer une action (`libs/database-entity/src/dto.rs:698`, lecture seule dans `libs/database/src/workspace.rs`). | `DEFAULT 0`, jamais fixé explicitement à l'insertion (`libs/database/src/workspace.rs:53-60`). |
| `af_workspace_ai_usage` (table) | `20240618035048_af_workspace_ai_usage.sql:1-8` | Compteurs journaliers d'usage IA/recherche (`search_requests`, `search_tokens_consumed`, `index_tokens_consumed`) — **compteur de télémétrie, pas de quota** (pas de colonne max). Écrite dans `libs/database/src/index/search_ops.rs:20-27` et par la procédure stockée `af_collab_embeddings_upsert`. **Jamais lue** (aucun `SELECT` dans `src/`/`libs/`) — écrite pour un service de billing externe qui n'est pas dans ce dépôt. | — |
| `af_workspace.settings` (JSONB) | `20240604090043_add_workspace_settings.sql:2` | Malgré son nom, ne contient que `disable_search_indexing` (bool) et `ai_model` (string, défaut `"Auto"`) — **rien à voir avec la facturation** (`libs/database-entity/src/dto.rs:707-722`). | Défaut Rust `ai_model="Auto"`, pas de défaut SQL. |
| `SubscriptionPlan` enum (Free/Pro/Team/AiMax/AiLocal) | `libs/shared-entity/src/dto/billing_dto.rs:36-43` | Le vrai enum de palier de plan — **mais uniquement échangé par HTTP avec un service de facturation externe**, jamais persisté en Postgres dans ce dépôt. | n/a (pas de DB) |
| `WorkspaceUsageAndLimit` (struct) | `billing_dto.rs:112-129` | Le struct le plus pertinent pour cette section : `member_count_limit`, `storage_bytes_limit`, `single_upload_limit`, `ai_responses_count_limit`, `ai_image_responses_count_limit`, `local_ai`. Récupéré via `Client::get_workspace_usage_and_limit()` (`libs/client-api/src/http_billing.rs:123-140`) qui appelle `GET {base_url}/api/workspace/{workspace_id}/usage-and-limit`. **Cette route n'a aucun handler serveur dans `src/`** (confirmé par grep) — le calcul et l'application de ces limites se font entièrement dans un service non inclus dans ce dépôt. | n/a |
| `SubscriptionCancelRequest`, `SubscriptionLinkRequest`, `SubscriptionPlanDetail`, `SubscriptionTrialRequest`, `LicensedProductType`, `SubscribeProductLicense` | `billing_dto.rs:131-231` | DTOs de requêtes Stripe (checkout, portail client, annulation, essai) et de licences desktop (Keygen-style) — tous purement des types d'échange HTTP avec des services externes, sans contrepartie DB. | n/a |

**Confirmation qu'il n'y a aucune route de billing server-side dans ce dépôt** : `grep -rn "billing" src --include="*.rs"` → zéro résultat. `src/api/` n'enregistre jamais de route `/billing/*` ou `/usage-and-limit`.

---

## 5. Synchronisation temps réel (CRDT)

### 5.1 Deux protocoles WebSocket coexistants

- `GET /ws/v1` (legacy actuel) → `establish_ws_connection_v1` (`src/api/ws.rs:67-106`), acteur actix `RealtimeClient` (`services/appflowy-collaborate/src/actix_ws/client/rt_client.rs:37`).
- `GET /ws/v2/{workspace_id}` (courant) → `establish_ws_connection_v2` (`ws.rs:109-160`), acteur `WsSession` (`services/appflowy-collaborate/src/ws2/actors/session.rs`).
- Auth : JWT bearer décodé via `authorization_from_token` avant upgrade WS (`ws.rs:119,175`).
- Rate limiting par connexion : `governor`, 10 msg/s par défaut (`rt_client.rs:36,66,98-111`).
- Gating de version client : `APPFLOWY_WEBSOCKET_CLIENT_MIN_VERSION` (`src/config/config.rs:223`).

### 5.2 Protocole / format des messages

Défini dans `libs/collab-rt-entity/src/message.rs`. Enveloppe `RealtimeMessage` (encodage **bincode**) :
```rust
pub enum RealtimeMessage {
  Collab(CollabMessage),                    // Deprecated
  User(UserMessage),
  System(SystemMessage),                    // RateLimit, KickOff, DuplicateConnection
  ClientCollabV1(Vec<ClientCollabMessage>),  // Deprecated
  ClientCollabV2(MessageByObjectId),         // courant : map object_id -> messages
  ServerCollabV1(Vec<ServerCollabMessage>),
}
```
`ClientCollabMessage` (`libs/collab-rt-entity/src/client_message.rs:29-35`) porte les frames du protocole de sync **yrs** (`ClientInitSync`, `ClientUpdateSync`, `ServerInitSync`, `ClientAwarenessSync`) — SyncStep1/SyncStep2/Update/Awareness, encodés avec `EncoderV1`.

### 5.3 Lien avec la lib externe `collab`

Dépendance **git pinnée** (`Cargo.toml:309-317`) :
```
collab = { git = "https://github.com/AppFlowy-IO/AppFlowy-Collab", rev = "e59260e524f33104b0ddcd6bb8f6218cad0f7e18" }
```
(idem pour `collab-entity`, `collab-folder`, `collab-document`, `collab-database`, `collab-user`, `collab-importer`, `collab-plugins`). Le cœur CRDT (portage Rust des concepts Yjs, basé sur `yrs`) vient donc du dépôt sœur `AppFlowy-IO/AppFlowy-Collab`, pas d'une crate locale. Les couches protocole/sync temps réel (`libs/collab-rt-entity`, `libs/collab-rt-protocol`, `libs/collab-stream`) sont, elles, locales à ce dépôt.

### 5.4 Rooms et diffusion

- **v1** : `GroupManager` (`services/appflowy-collaborate/src/group/manager.rs:24-54`) maintient une `CollabGroup` par `object_id` ("room"). Les mises à jour client sont d'abord poussées dans un **stream Redis** (`CollabPersister::send_update`), puis un `inbound_task` séparé consomme ce stream et les diffuse aux autres abonnés (`group_init.rs:177-320`) — ce relais Redis permet à plusieurs instances du serveur de rester synchronisées.
- **v2** : `WsServer` maintient un acteur `Workspace` par `workspace_id` (room par workspace, pas par document), même pattern de relais Redis via `CollabManager` (`services/appflowy-collaborate/src/collab/collab_manager.rs`), avec curseur `Rid` (Redis stream id) pour reprise après reconnexion.

### 5.5 Persistance Postgres et snapshots

- Table `af_collab` (§4) — blobs bruts. `CollabStoreImpl` (`services/appflowy-collaborate/src/collab/collab_store.rs:43-338`) compose un cache mémoire + `CollabDiskCache`, avec vérification d'accès avant chaque lecture/écriture.
- Snapshots : deux ordonnanceurs indépendants (v1 `CollabGroup::snapshot_task`, intervalle `APPFLOWY_COLLAB_GROUP_PERSISTENCE_INTERVAL` défaut 60s, `group_init.rs:322-353` ; v2 `SnapshotScheduler`, `services/appflowy-collaborate/src/collab/snapshot_scheduler.rs`, 60s également) — écrivent exclusivement sur **S3**, pas Postgres, compressés zstd, avec purge au-delà de `COLLAB_SNAPSHOT_LIMIT` (`services/appflowy-collaborate/src/snapshot/snapshot_control.rs:82-118`).
- Offload S3 des gros documents : au-delà de `APPFLOWY_COLLAB_S3_THRESHOLD` (défaut 8000 octets, `src/config/config.rs:255`), le blob collab est stocké sur S3 au lieu de Postgres (`services/appflowy-collaborate/src/collab/cache/disk_cache.rs:110-139`).

---

## 6. ⚠️ Application des limites de plan

### 6.1 Méthodologie

Grep exhaustif sur `subscription|plan|quota|limit|workspace_settings|billing|stripe|ai_credit|seat` sur l'ensemble du dépôt (`src/`, `libs/`, `services/`, `admin_frontend/`, `migrations/`), puis lecture ciblée de chaque point d'entrée métier susceptible d'appliquer une limite : invitation de membres (`src/biz/workspace/ops.rs:357-436`), upload de fichiers (`src/api/file_storage.rs`), sélection de modèle IA (`src/api/util.rs:214-220`), settings de workspace (`src/biz/workspace/ops.rs:640-667`).

### 6.2 Résultat, point par point

| Limite cherchée | Existe-t-elle dans ce dépôt ? | Détail |
|---|---|---|
| **Nombre de membres/sièges** | ❌ Non appliquée | `invite_workspace_members` (`src/biz/workspace/ops.rs:357-436`) calcule `workspace_member_count` (`:374-377`) mais l'utilise **uniquement comme texte dans l'email d'invitation** (`:400`, interpolé dans le template). Aucune comparaison à une limite, aucun rejet. |
| **Quota de stockage (storage_bytes_limit)** | ❌ Non appliquée | `src/api/file_storage.rs` gère `content_length` par requête individuelle mais n'agrège jamais l'usage total du workspace contre une limite. `af_blob_metadata.file_size` (§4) existe mais n'est sommé/comparé nulle part dans `src/`/`libs/`. Seule limite réelle : `MAX_BODY_SIZE` par requête (`src/api/workspace.rs:2854-2859`), une limite technique de taille de payload HTTP, pas un quota de plan. |
| **Limite de taille d'upload unique (`single_upload_limit`)** | ❌ Non appliquée | Champ présent dans `WorkspaceUsageAndLimit` (DTO client uniquement, §4.2) ; aucun contrôle côté serveur dans ce dépôt. |
| **Quota de réponses IA (`ai_responses_count_limit`)** | ❌ Non appliquée | `src/api/ai.rs` et `src/biz/chat/ops.rs` : zéro occurrence de `limit`/`quota`/`exceed`/`credit`. Les seuls compteurs sont des métriques Prometheus **non lues en retour** (`src/biz/chat/metrics.rs:6-93`, incrémentées sans jamais être comparées à un seuil, ex. `src/api/ai.rs:42,82`). Le choix de modèle IA vient d'un simple header client `ai-model`, sans vérification de plan (`src/api/util.rs:214-220`). |
| **`local_ai` (accès aux modèles IA locaux)** | ❌ Non gérée côté serveur | `local_ai_config_handler` (`src/api/ai.rs:168-...`) proxifie directement vers le service externe AI (`/local_ai/config`), sans vérification de plan. |
| **`workspace_type` (palier "Free"/etc.)** | ⚠️ Existe en DB mais **inerte** | Colonne `af_workspace.workspace_type` (§4.2) : jamais lue pour conditionner un comportement dans `src/` ou `libs/`. Purement informationnel/passthrough. |
| **Endpoints de facturation (Stripe, etc.)** | ❌ Absents de ce dépôt | Zéro route `/billing`, `/subscription`, `/usage-and-limit` dans `src/api/`. Les DTOs existent (`libs/shared-entity/src/dto/billing_dto.rs`) et un client HTTP les appelle (`libs/client-api/src/http_billing.rs`), mais **pointent vers un service de facturation externe non présent dans ce dépôt**. |
| **`SIGNUP_WHITELIST_ENABLED` / `GUEST_INVITES_REQUIRE_ADMIN_APPROVAL`** | ⚠️ Déclarées mais **inertes dans ce snapshot** | Ces deux variables apparaissent dans `docker-compose.yml:154-160` avec des commentaires explicites décrivant leur intention (liste blanche de signup, approbation admin des invités) — mais un grep de tout `src/`, `libs/`, `admin_frontend/` (`*.rs`) pour `SIGNUP_WHITELIST` / `GUEST_INVITES_REQUIRE` ne renvoie **aucun résultat**. Ce sont des réglages vestigiaux (probablement câblés uniquement dans le fork commercial fermé, ou dans une version plus récente de `appflowy_cloud` que ce snapshot ne contient pas) — **sans effet réel dans ce code source**. |
| **`GOTRUE_DISABLE_SIGNUP`** | ✅ Fonctionne réellement, mais ce n'est **pas une limite de plan** | Contrôlée par le binaire GoTrue lui-même (externe, closed-source Go), pas par ce dépôt Rust. C'est un vrai levier d'auto-hébergement (couper les inscriptions publiques), indépendant de toute logique d'abonnement. |

### 6.3 Conclusion

**Sur une instance self-hostée "out of the box" (ce dépôt, sans clé Stripe, sans service de facturation externe déployé) : aucune limite de plan payant ne s'applique.** Il n'y a rien à patcher pour les "lever" au sens propre, car **elles ne sont tout simplement pas implémentées dans ce code source** :

- Le nombre de sièges/membres, le stockage, les quotas IA, ne sont vérifiés nulle part côté serveur.
- Les seuls DTOs de plan (`SubscriptionPlan`, `WorkspaceUsageAndLimit`, etc.) sont des types d'échange HTTP avec un microservice de facturation propriétaire qui **ne fait pas partie de ce dépôt** — il faudrait le déployer soi-même (ce que personne ne fait en self-host AGPLv3) pour que la moindre restriction s'applique.
- La seule colonne DB ressemblant à un palier de plan (`af_workspace.workspace_type`) est écrite à `0` par défaut et jamais lue pour bloquer quoi que ce soit.
- Les deux variables d'env qui *semblent* gater signup/invités (`SIGNUP_WHITELIST_ENABLED`, `GUEST_INVITES_REQUIRE_ADMIN_APPROVAL`) sont mortes dans ce code — ni à activer, ni à patcher, elles n'ont simplement aucun effet.

Il n'y a donc **aucun patch de contournement à écrire** pour ce dépôt précis : la limitation « 1 siège gratuit » etc. n'existe que dans le **fork commercial fermé** (`AppFlowy-SelfHost-Commercial`), qui n'est pas ce que ce document analyse et qui n'est de toute façon pas accessible pour rétro-ingénierie ici. Voir §9 pour la synthèse actionnable.

---

## 7. Intégration IA

Le backend est un **simple proxy** — il ne parle pas directement à OpenAI/Azure pour le chat/complétion (contrairement à la recherche, voir plus bas).

### 7.1 Chemin de la requête
- `src/api/ai.rs` (complétion/embeddings) et `src/api/chat.rs` (conversations) récupèrent `state.ai_client: AppFlowyAIClient` (`src/state.rs:61`) et transmettent la requête telle quelle, ex. `state.ai_client.stream_completion_text(params, ai_model)` (`src/api/ai.rs:55-58`).
- `AppFlowyAIClient` (`libs/appflowy-ai-client/src/client.rs:22-347`) est un client `reqwest` HTTP simple (pas gRPC) vers le service externe **AppFlowy AI Gateway**, construit au démarrage (`src/application.rs:245`).
- URL du service : `AppFlowyAISetting::url()` = `http://{host}:{port}`, vars `AI_SERVER_HOST`/`AI_SERVER_PORT` (défaut `localhost:5001`, `src/config/config.rs:82-88,241-244`) — dans docker-compose, pointe vers le conteneur `ai` sœur.
- Sélection du modèle : header client `ai-model` (`src/api/util.rs:214-220`, défaut `"Default"`), transmis tel quel au service IA.

### 7.2 Clés API et providers
- **Ce dépôt ne stocke aucune clé de provider IA pour le chat/complétion** — c'est le service externe `ai` (image `appflowyinc/appflowy_ai`, closed-source) qui détient `OPENAI_API_KEY`/`AZURE_OPENAI_*` (`docker-compose.yml:189-194`).
- Clés OpenAI/Azure lues **directement** dans ce dépôt, mais **uniquement pour la recherche sémantique/indexation** (pgvector), pas pour le chat : `AI_OPENAI_API_KEY`, `AI_AZURE_OPENAI_API_KEY/API_BASE/API_VERSION` → `get_open_ai_config()` (`libs/indexer/src/vector/embedder.rs:35-66`), utilisé par `libs/indexer/src/scheduler.rs` et `create_ai_tool` (`src/api/search.rs:70-84`).
- Modèles IA locaux/offline : proxifiés également, `AppFlowyAIClient::get_local_ai_config` (`libs/appflowy-ai-client/src/client.rs:289-317`) → routes externes `/local_ai/config`, `/local_ai/plugin`. La liste des modèles disponibles (`/model/list`) est dynamique côté service externe, pas codée en dur dans ce dépôt.

### 7.3 Facturation/comptage
**Aucun mécanisme de comptage/limitation d'usage IA trouvé dans le code de ce dépôt.** Seule instrumentation : compteurs Prometheus `AIMetrics` (`src/biz/chat/metrics.rs:6-15`, `total_stream_count`, `total_completion_count`, `prompt_usage_count` par `prompt_id`) — incrémentés systématiquement (`src/api/ai.rs:42,52,82`) mais **jamais relus pour bloquer une requête**. Toute logique de facturation/quota IA, si elle existe, vit dans le service externe AppFlowy AI Gateway ou dans le service de billing séparé — ni l'un ni l'autre n'est présent dans ce dépôt.

---

## 8. Déploiement

### 8.1 Services `docker-compose.yml`

| Service | Image | Port publié | Dépend de |
|---|---|---|---|
| `nginx` | `nginx` | `${NGINX_PORT:-80}`, `${NGINX_TLS_PORT:-443}` | — |
| `minio` | `minio/minio` | aucun (via nginx) | — |
| `postgres` | `pgvector/pgvector:pg16` | aucun | — |
| `redis` | `redis` | aucun | — |
| `gotrue` | `appflowyinc/gotrue` | aucun | `postgres` (healthy) |
| `appflowy_cloud` | `appflowyinc/appflowy_cloud` | aucun (derrière nginx) | `gotrue` (healthy) |
| `admin_frontend` | `appflowyinc/admin_frontend` | aucun | `gotrue`, `appflowy_cloud` (healthy) |
| `ai` | `appflowyinc/appflowy_ai` | aucun | `postgres`, `appflowy_cloud` (healthy) |
| `appflowy_worker` | `appflowyinc/appflowy_worker` | aucun | `postgres`, `appflowy_cloud` (healthy) |
| `appflowy_search` | `appflowyinc/appflowy_search` | aucun | `postgres` (healthy) |
| `appflowy_web` | `appflowyinc/appflowy_web` | aucun | `appflowy_cloud` (healthy) |

Seuls `appflowy_cloud`, `admin_frontend` et `appflowy_worker` sont buildés depuis le code source **de ce dépôt** (`Dockerfile`, `services/appflowy-worker/Dockerfile`) ; `gotrue`, `ai`, `appflowy_search`, `appflowy_web` sont des images **pré-construites et closed-source** tirées d'autres dépôts AppFlowy.

### 8.2 Variables d'environnement

**Obligatoires (sans quoi le service ne démarre pas ou l'auth ne fonctionne pas)** :
- `APPFLOWY_WEB_URL` — seule variable qui fait échouer `get_configuration()` si absente (`src/config/config.rs:274-275`, `.ok_or(anyhow!(...))?`).
- Accessibilité **Postgres** au boot (migrations auto au démarrage, `src/application.rs:538-556`) et **Redis** (store de session, `src/application.rs:111-119`) — sinon le process ne démarre pas.
- Côté `docker-compose.yml` : `GOTRUE_ADMIN_EMAIL`, `GOTRUE_ADMIN_PASSWORD`, `GOTRUE_JWT_SECRET` — pas de défaut compose (`docker-compose.yml:70-77`), GoTrue peut refuser de démarrer sans eux.
- `GoTrue` lui-même : health-check non bloquant côté `appflowy_cloud` (`let _ =` avale l'erreur, `src/application.rs:558-566`) — le serveur démarre même si GoTrue est down, mais l'auth ne fonctionnera pas.

**Optionnelles (défaut de code, ou fonctionnalité simplement désactivée)** :
- Quasiment tout `src/config/config.rs:179-294` a un défaut inline : `APPFLOWY_DATABASE_URL` (`postgres://postgres:password@localhost:5432/postgres`), `APPFLOWY_GOTRUE_JWT_SECRET` (`hello456` — **à changer impérativement en production**), `APPFLOWY_REDIS_URI` (`redis://localhost:6379`), `APPFLOWY_S3_*` (défauts MinIO `minioadmin`/`minioadmin`).
- `AI_OPENAI_API_KEY`/`AI_AZURE_OPENAI_*` — absents = fonctionnalités IA désactivées, reste du produit pleinement opérationnel (`deploy.env:238`).
- `APPFLOWY_MAILER_SMTP_*` — pas de connexion testée au boot (`libs/mailer/src/sender.rs:18-45`), échec silencieux différé à l'envoi ; notifications email en plus gatées par `APPFLOWY_NOTIFICATION_ENABLE_EMAIL` (défaut `false`).
- `APPFLOWY_ACCESS_CONTROL` — bascule vers des implémentations `NoOps*` si désactivé (`src/application.rs:274-301`).
- Providers OAuth (Google/GitHub/Discord/SAML) — chacun désactivé par défaut (`*_ENABLED=false`).

### 8.3 Prérequis pour un déploiement local de bout en bout

1. `docker-compose.yml` seul suffit pour la stack "essentielle" (commentaire en tête de fichier) ; `docker-compose-extras.yml` ajoute Cloudflare Tunnel + pgadmin en option (`include`). `docker-compose-dev.yml` est réservé au dev natif (`cargo run` contre une infra nue).
2. Génération des env : copier `env.deploy.secret.example` → `.env.deploy.secret` (ou `env.dev.secret.example` → `.env.dev.secret` en dev), puis exécuter `./script/generate_env.sh` qui fusionne `deploy.env`/`dev.env` avec le fichier de secrets en un `.env` final.
3. Valeurs à changer impérativement des défauts avant tout déploiement réel : `GOTRUE_JWT_SECRET` (placeholder `hello456`), `GOTRUE_ADMIN_EMAIL`/`PASSWORD` (compte admin bootstrap), `POSTGRES_PASSWORD`, clés MinIO/S3 (`minioadmin`/`minioadmin` explicitement signalées "should never be used in production" — `deploy.env:45-51`), `FQDN`/`SCHEME`/`WS_SCHEME`.
4. TLS : certificats attendus à `nginx/ssl/certificate.crt` / `nginx/ssl/private_key.key` si HTTPS servi par le nginx fourni.
5. Ordre de démarrage imposé par `depends_on: condition: service_healthy` : `postgres`/`gotrue` → `appflowy_cloud` → `admin_frontend`/`ai`/`appflowy_worker`/`appflowy_web`.
6. Migrations Postgres exécutées automatiquement à chaque démarrage du conteneur `appflowy_cloud` — aucune étape manuelle nécessaire en déploiement compose.

---

## 9. Résumé exécutif

**Question : que faut-il modifier, et où, pour self-hoster cette stack sans aucune limitation de plan payant, pour un usage personnel ?**

### Réponse courte
**Rien.** Le code de ce dépôt (`AppFlowy-IO/AppFlowy-Cloud`, AGPLv3) — celui que vous obtenez en le clonant et en lançant `docker-compose.yml` — n'implémente **aucune** limite de plan payant côté serveur : pas de vérification de nombre de sièges, pas de quota de stockage, pas de plafond de réponses IA, pas de route de facturation. Ces contrôles (le palier "Free : 1 siège, 3 invités") n'existent que dans le **fork commercial fermé** `AppFlowy-IO/AppFlowy-SelfHost-Commercial`, qui n'est pas ce que vous déployez si vous partez de ce dépôt open-source. Il n'y a donc **rien à patcher dans `src/`, `libs/` ou les migrations** pour "débloquer" quoi que ce soit — les DTOs `SubscriptionPlan`/`WorkspaceUsageAndLimit` (`libs/shared-entity/src/dto/billing_dto.rs`) existent dans le code mais ne sont jamais consultés pour restreindre une action : ce sont des coquilles vides côté serveur, prévues pour dialoguer avec un microservice de facturation propriétaire que vous ne déployez pas.

### Ce qui reste à faire pour un déploiement personnel propre (pas des "patchs anti-limite", mais de l'hygiène de config)

1. **Changer les secrets par défaut** avant toute exposition réseau : `GOTRUE_JWT_SECRET` (défaut `hello456`, `src/config/config.rs:213` / `deploy.env:104`), identifiants MinIO/S3 (défaut `minioadmin`/`minioadmin`), `GOTRUE_ADMIN_EMAIL`/`PASSWORD`, `POSTGRES_PASSWORD`.
2. **Décider de la politique de signup** via les vrais leviers qui fonctionnent : `GOTRUE_DISABLE_SIGNUP` (côté binaire GoTrue, §6) — pour un usage strictement personnel, mettez-le à `true` une fois votre compte créé, afin d'éviter que votre instance ne serve de service public ouvert. Ignorez `SIGNUP_WHITELIST_ENABLED`/`GUEST_INVITES_REQUIRE_ADMIN_APPROVAL` : ils sont déclarés dans `docker-compose.yml` mais **morts dans le code Rust de ce snapshot** — inutile de les configurer, ils n'ont aucun effet.
3. **IA (optionnel)** : fournissez votre propre clé `AI_OPENAI_API_KEY` (ou Azure) au conteneur `ai` (`docker-compose.yml:189-194`) et à l'indexeur (`libs/indexer`) — coût à votre charge, mais **sans plafond applicatif** puisqu'aucune limite n'est vérifiée côté `appflowy_cloud` (§7).
4. **Ne déployez pas de service de facturation externe** — c'est la seule chose qui pourrait introduire une restriction, et elle n'est pas incluse ici.
5. **Vérifiez périodiquement** que vous suivez bien ce dépôt (`AppFlowy-IO/AppFlowy-Cloud`) et non un fork qui réintroduirait des vérifications de plan — le README (`README.md:22-51`) documente explicitement que les deux offres commerciales sont maintenues *séparément*, donc les futures limitations éventuelles apparaîtraient dans ce dépôt seulement si l'équipe AppFlowy changeait sa stratégie open-core, ce qui vaut la peine d'être surveillé (CHANGELOG/README) à chaque mise à jour de version.

### En une phrase
Ce dépôt open-source est déjà, tel quel, un AppFlowy Cloud « sans limite » — la seule vigilance nécessaire porte sur la configuration (secrets, politique de signup), pas sur du patch de contournement de quota, puisqu'aucun quota n'y est câblé.
