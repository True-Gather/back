BEGIN;

-- =====================================================
-- ENUM: group_member_role
-- =====================================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_type
        WHERE typname = 'group_member_role'
    ) THEN
        CREATE TYPE group_member_role AS ENUM ('owner', 'admin', 'member');
    END IF;
END
$$;

-- Si la colonne role de group_members est encore en TEXT,
-- on la convertit proprement vers l'enum.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'group_members'
          AND column_name = 'role'
          AND data_type <> 'USER-DEFINED'
    ) THEN

        ALTER TABLE group_members
        ALTER COLUMN role DROP DEFAULT;

        ALTER TABLE group_members
        ALTER COLUMN role TYPE group_member_role
        USING role::group_member_role;

        ALTER TABLE group_members
        ALTER COLUMN role SET DEFAULT 'member';

    END IF;
END
$$;

-- =====================================================
-- ENUM: group_invitation_status
-- =====================================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_type
        WHERE typname = 'group_invitation_status'
    ) THEN
        CREATE TYPE group_invitation_status AS ENUM (
            'pending',
            'accepted',
            'declined',
            'cancelled'
        );
    END IF;
END
$$;

-- =====================================================
-- TABLE: group_invitations
-- =====================================================
CREATE TABLE IF NOT EXISTS group_invitations (
    group_invitation_id UUID PRIMARY KEY,
    group_id UUID NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE,
    invited_user_keycloak_id TEXT NOT NULL REFERENCES users(keycloak_id) ON DELETE CASCADE,
    invited_by_user_keycloak_id TEXT NOT NULL REFERENCES users(keycloak_id) ON DELETE CASCADE,
    status group_invitation_status NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    responded_at TIMESTAMPTZ NULL,
    cancelled_at TIMESTAMPTZ NULL,

    CONSTRAINT uq_group_pending_invitation
    UNIQUE (group_id, invited_user_keycloak_id, status)
);

-- On supprime ensuite le problème du UNIQUE avec status multiple,
-- en remplaçant par un index partiel plus propre.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'uq_group_pending_invitation'
    ) THEN
        ALTER TABLE group_invitations
        DROP CONSTRAINT uq_group_pending_invitation;
    END IF;
END
$$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_group_invitations_one_pending
ON group_invitations (group_id, invited_user_keycloak_id)
WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_group_invitations_invited_user
ON group_invitations (invited_user_keycloak_id);

CREATE INDEX IF NOT EXISTS idx_group_invitations_group
ON group_invitations (group_id);

-- =====================================================
-- TABLE: notifications
-- Si elle existe déjà, on l'étend.
-- =====================================================

ALTER TABLE notifications
ADD COLUMN IF NOT EXISTS notification_type TEXT NOT NULL DEFAULT 'generic';

ALTER TABLE notifications
ADD COLUMN IF NOT EXISTS related_group_id UUID NULL REFERENCES groups(group_id) ON DELETE CASCADE;

ALTER TABLE notifications
ADD COLUMN IF NOT EXISTS related_group_invitation_id UUID NULL REFERENCES group_invitations(group_invitation_id) ON DELETE CASCADE;

ALTER TABLE notifications
ADD COLUMN IF NOT EXISTS action_status TEXT NULL;

CREATE INDEX IF NOT EXISTS idx_notifications_user_created_at
ON notifications (user_keycloak_id, created_at DESC);

COMMIT;