-- =====================================================
-- Migration 0002 : vérification d'email
--
-- Ajoute :
--   1. email_verified BOOLEAN sur la table users
--   2. Table email_verification_tokens (backup / audit)
--      Note : le stockage opérationnel des tokens est dans Redis
--             (clé email_verify:{hash}, TTL 15 min).
--             Cette table sert de journal immuable.
-- =====================================================

-- 1. Champ email_verified (ne recule jamais en arrière une fois true)
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS email_verified BOOLEAN NOT NULL DEFAULT FALSE;

-- 2. Table des tokens de vérification
--    Le token brut est envoyé dans l'email.
--    Seul son hash SHA-256 est stocké ici (jamais le token en clair).

CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID        NOT NULL,
    token_hash      VARCHAR(64) NOT NULL,
    expires_at      TIMESTAMPTZ NOT NULL,
    used_at         TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Un seul token actif par utilisateur à la fois
    CONSTRAINT uq_email_verification_token_user
        UNIQUE (user_id),

    CONSTRAINT uq_email_verification_token_hash
        UNIQUE (token_hash)
);

-- Index pour lookup rapide par hash (chemin de vérification)
CREATE INDEX IF NOT EXISTS idx_evt_hash
    ON email_verification_tokens(token_hash)
    WHERE used_at IS NULL;

-- Index pour invalider les anciens tokens d'un utilisateur
CREATE INDEX IF NOT EXISTS idx_evt_user
    ON email_verification_tokens(user_id)
    WHERE used_at IS NULL;
