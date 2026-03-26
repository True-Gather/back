# Truegather - Backend

Bienvenue dans la partie **Backend** du projet Truegather ! 🚀
Ce projet est construit en **Rust** (avec le framework web **Axum**). Ce document a pour but de vous guider pas à pas, surtout si vous débutez.

## 🛠 Prérequis

Avant de commencer, assurez-vous d'avoir installé les outils suivants sur votre machine :
1. **[Rust et Cargo](https://rustup.rs/)** : Le langage de programmation et son gestionnaire de paquets/build.
2. **[Docker & Docker Compose](https://docs.docker.com/get-docker/)** : Pour lancer facilement notre base de données, notre serveur d'authentification (Keycloak) et Redis.
3. **Git** : Pour la gestion de version.

## 🚀 Démarrage rapide (Développement Local)

L'avantage de Docker Compose est qu'il vous permet de lancer toute l'infrastructure (Base de données, Keycloak, Redis) sans rien installer manuellement sur votre PC.

### 1. Lancer l'infrastructure avec Docker Compose
Le fichier `docker-compose.yml` contient la configuration de tous nos services externes.
Pour démarrer Postgres (base de données), Redis (cache) et Keycloak (authentification) :
```bash
# Dans le dossier backend/
docker compose up -d
```
*L'option `-d` lance les conteneurs en tâche de fond (detached).*

**Note sur Keycloak :**
Notre configuration Docker injecte automatiquement la configuration de démarrage (le *realm* Truegather) depuis le dossier `keycloak/import` et le thème visuel depuis `keycloak/themes`.

### 2. Configurer les variables d'environnement
Créez un fichier `.env` à la racine du dossier `backend` (s'il n'existe pas déjà) en vous inspirant d'un `.env.example`.
Il devrait contenir les accès à Keycloak et Redis, par exemple :
```env
RUST_LOG=info
KEYCLOAK_URL=http://localhost:8081
REDIS_URL=redis://localhost:6379
```

### 3. Lancer le serveur Rust
Maintenant que l'infrastructure tourne, compilez et lancez le backend :
```bash
cargo run
```
*Lors du premier lancement, la compilation peut prendre un peu de temps. C'est normal !*

---

## 🏗 Architecture et Fichiers Clés

- `src/` : Contient tout le code source Rust.
  - `main.rs` : Le point d'entrée de notre API.
  - `api/` & `auth/` & `models/` : Nos contrôleurs (handlers), middlewares et modèles de données.
- `Cargo.toml` : La liste de nos dépendances (Axum, Tokio, etc.). C'est l'équivalent du `package.json` en NodeJS.
- `Dockerfile` : Les instructions pour packager notre code Rust en une image Docker légère pour la production.
- `docker-compose.yml` : Le chef d'orchestre de nos conteneurs (Backend, Keycloak, Postgres, Redis).
- `.github/workflows/ci-cd.yml` : Notre pipeline d'intégration (CI) et de déploiement continu (CD).

---

## 🐳 Docker et Production

Si vous souhaitez tester l'image Docker finale du Backend (comme elle sera en production) :

```bash
# Construire l'image du backend
docker build -t truegather-backend .

# La lancer avec docker run ou simplement via docker-compose :
# (assurez-vous d'avoir arrêté `cargo run` avant)
docker compose up -d backend
```
Le `Dockerfile` utilise la méthode "multi-stage build" :
1. **Étape 1 (Builder)** : Utilise l'image officielle lourde de Rust pour compiler notre code.
2. **Étape 2 (Runtime)** : Ne garde qu'une image `debian` ultra légère et le binaire compilé (`truegather-backend`), ce qui réduit drastiquement la taille et augmente la sécurité.

---

## 🤖 CI / CD (Intégration et Déploiement Continus)

Le dossier `.github/workflows/` contient `ci-cd.yml`. C'est un processus automatisé qui tourne sur les serveurs de GitHub (GitHub Actions) à chaque fois que vous "Poussez" (`git push`) du code sur la branche `main`.

**Que fait la CI/CD ?**
1. **Vérification** : Elle formate votre code (`cargo fmt`), vérifie les erreurs courantes (`cargo clippy`).
2. **Tests** : Elle lance tous les tests de l'application (`cargo test`).
3. **Déploiement** : Si (et seulement si) tout est vert, elle construit l'image Docker de production et la publie sur le registre public **DockerHub**.

---

## 💡 Commandes Utiles au quotidien

- Vérifier que votre code compile : `cargo check`
- Lancer le linter pour les bonnes pratiques : `cargo clippy`
- Afficher les logs des conteneurs : `docker compose logs -f`
- Arrêter l'infrastructure : `docker compose down`

Bon code ! 🎉
