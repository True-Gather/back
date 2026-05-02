-- =====================================================
-- Migration 0003 : sessions en PostgreSQL + UUID sur users
--
-- 1. Ajoute une colonne id UUID à la table users
--    (les utilisateurs seront identifiés par UUID en interne,
--     keycloak_id reste la PK externe)
-- 2. Crée la table sessions (remplace Redis pour les sessions auth)
-- =====================================================

-- 1. Colonne id UUID sur users
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS id UUID DEFAULT gen_random_uuid();

-- Remplir les lignes existantes
UPDATE users SET id = gen_random_uuid() WHERE id IS NULL;

-- Rendre obligatoire + index unique
ALTER TABLE users ALTER COLUMN id SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_uuid
    ON users(id);

-- 2. Table sessions applicatives
--    Remplace les entrées Redis tg:session:{session_id}
--    TTL géré par la colonne expires_at (nettoyage par le backend au démarrage)
CREATE TABLE IF NOT EXISTS sessions (
    session_id    TEXT         PRIMARY KEY,
    user_id       UUID         NOT NULL,
    keycloak_sub  TEXT         NOT NULL,
    email         TEXT         NOT NULL,
    display_name  TEXT         NOT NULL,
    first_name    TEXT,
    last_name     TEXT,
    id_token      TEXT,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    expires_at    TIMESTAMPTZ  NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_expires_at
    ON sessions(expires_at);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id
    ON sessions(user_id);

-- 3. Nettoyage des sessions expirées (fonction utilitaire appelable manuellement)
CREATE OR REPLACE FUNCTION cleanup_expired_sessions()
RETURNS INTEGER AS $$
DECLARE
    deleted INTEGER;
BEGIN
    DELETE FROM sessions WHERE expires_at < NOW();
    GET DIAGNOSTICS deleted = ROW_COUNT;
    RETURN deleted;
END;
$$ LANGUAGE plpgsql;
