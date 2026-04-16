-- V1 groupes :
-- - photo optionnelle,
-- - date de mise à jour,
-- - rôle dans le groupe,
-- - date d'ajout du membre,
-- - unicité groupe + user.

ALTER TABLE groups
ADD COLUMN IF NOT EXISTS profile_photo_url TEXT;

ALTER TABLE groups
ADD COLUMN IF NOT EXISTS updated_at TIMESTAMP NOT NULL DEFAULT NOW();

ALTER TABLE group_members
ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'member';

ALTER TABLE group_members
ADD COLUMN IF NOT EXISTS joined_at TIMESTAMP NOT NULL DEFAULT NOW();

CREATE UNIQUE INDEX IF NOT EXISTS idx_group_members_group_user_unique
ON group_members (group_id, user_keycloak_id);