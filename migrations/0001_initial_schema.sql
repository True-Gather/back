-- =====================================================
-- TABLE: users
-- Les utilisateurs viennent de Keycloak
-- => keycloak_id reste en VARCHAR(255)
-- =====================================================
CREATE TABLE IF NOT EXISTS users (
    keycloak_id VARCHAR(255) PRIMARY KEY,
    first_name VARCHAR(100) NOT NULL,
    last_name VARCHAR(100) NOT NULL,
    display_name VARCHAR(150),
    email VARCHAR(255) NOT NULL UNIQUE,
    profile_photo_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_login_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

-- =====================================================
-- TABLE: groups
-- IDs internes => UUID
-- =====================================================
CREATE TABLE IF NOT EXISTS groups (
    group_id UUID PRIMARY KEY,
    owner_keycloak_id VARCHAR(255) NOT NULL,
    name VARCHAR(150) NOT NULL,
    description TEXT,
    photo_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT fk_groups_owner
        FOREIGN KEY (owner_keycloak_id)
        REFERENCES users(keycloak_id)
        ON DELETE CASCADE
);

-- =====================================================
-- TABLE: group_members
-- Association entre groupes et utilisateurs
-- =====================================================
CREATE TABLE IF NOT EXISTS group_members (
    group_member_id UUID PRIMARY KEY,
    group_id UUID NOT NULL,
    user_keycloak_id VARCHAR(255) NOT NULL,
    added_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT fk_group_members_group
        FOREIGN KEY (group_id)
        REFERENCES groups(group_id)
        ON DELETE CASCADE,

    CONSTRAINT fk_group_members_user
        FOREIGN KEY (user_keycloak_id)
        REFERENCES users(keycloak_id)
        ON DELETE CASCADE,

    CONSTRAINT uq_group_member UNIQUE (group_id, user_keycloak_id)
);

-- =====================================================
-- TABLE: meetings
-- =====================================================
CREATE TABLE IF NOT EXISTS meetings (
    meeting_id UUID PRIMARY KEY,
    host_keycloak_id VARCHAR(255) NOT NULL,
    title VARCHAR(200) NOT NULL,
    description TEXT,
    meeting_type VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL,
    scheduled_start_at TIMESTAMPTZ,
    scheduled_end_at TIMESTAMPTZ,
    actual_start_at TIMESTAMPTZ,
    actual_end_at TIMESTAMPTZ,
    ai_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    meeting_link TEXT,
    room_code VARCHAR(100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT fk_meetings_host
        FOREIGN KEY (host_keycloak_id)
        REFERENCES users(keycloak_id)
        ON DELETE CASCADE,

    CONSTRAINT chk_meeting_type
        CHECK (meeting_type IN ('instant', 'scheduled')),

    CONSTRAINT chk_meeting_status
        CHECK (status IN ('scheduled', 'live', 'completed', 'cancelled')),

    CONSTRAINT chk_meeting_dates
        CHECK (
            scheduled_end_at IS NULL
            OR scheduled_start_at IS NULL
            OR scheduled_end_at >= scheduled_start_at
        ),

    CONSTRAINT chk_meeting_actual_dates
        CHECK (
            actual_end_at IS NULL
            OR actual_start_at IS NULL
            OR actual_end_at >= actual_start_at
        ),

    CONSTRAINT uq_room_code UNIQUE (room_code)
);

-- =====================================================
-- TABLE: meeting_participants
-- Association entre meetings et utilisateurs
-- =====================================================
CREATE TABLE IF NOT EXISTS meeting_participants (
    meeting_participant_id UUID PRIMARY KEY,
    meeting_id UUID NOT NULL,
    user_keycloak_id VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL,
    invited_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    joined_at TIMESTAMPTZ,
    left_at TIMESTAMPTZ,

    CONSTRAINT fk_meeting_participants_meeting
        FOREIGN KEY (meeting_id)
        REFERENCES meetings(meeting_id)
        ON DELETE CASCADE,

    CONSTRAINT fk_meeting_participants_user
        FOREIGN KEY (user_keycloak_id)
        REFERENCES users(keycloak_id)
        ON DELETE CASCADE,

    CONSTRAINT chk_meeting_participant_role
        CHECK (role IN ('host', 'participant')),

    CONSTRAINT chk_meeting_participant_status
        CHECK (status IN ('invited', 'joined', 'left', 'declined', 'absent')),

    CONSTRAINT chk_meeting_participant_times
        CHECK (
            left_at IS NULL
            OR joined_at IS NULL
            OR left_at >= joined_at
        ),

    CONSTRAINT uq_meeting_participant UNIQUE (meeting_id, user_keycloak_id)
);

-- =====================================================
-- TABLE: meeting_group_invites
-- Groupes invités à un meeting
-- =====================================================
CREATE TABLE IF NOT EXISTS meeting_group_invites (
    meeting_group_invite_id UUID PRIMARY KEY,
    meeting_id UUID NOT NULL,
    group_id UUID NOT NULL,
    invited_by_keycloak_id VARCHAR(255) NOT NULL,
    invited_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT fk_meeting_group_invites_meeting
        FOREIGN KEY (meeting_id)
        REFERENCES meetings(meeting_id)
        ON DELETE CASCADE,

    CONSTRAINT fk_meeting_group_invites_group
        FOREIGN KEY (group_id)
        REFERENCES groups(group_id)
        ON DELETE CASCADE,

    CONSTRAINT fk_meeting_group_invites_user
        FOREIGN KEY (invited_by_keycloak_id)
        REFERENCES users(keycloak_id)
        ON DELETE CASCADE,

    CONSTRAINT uq_meeting_group_invite UNIQUE (meeting_id, group_id)
);

-- =====================================================
-- TABLE: notifications
-- =====================================================
CREATE TABLE IF NOT EXISTS notifications (
    notification_id UUID PRIMARY KEY,
    user_keycloak_id VARCHAR(255) NOT NULL,
    type VARCHAR(50) NOT NULL,
    title VARCHAR(150) NOT NULL,
    message TEXT NOT NULL,
    related_meeting_id UUID,
    related_group_id UUID,
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT fk_notifications_user
        FOREIGN KEY (user_keycloak_id)
        REFERENCES users(keycloak_id)
        ON DELETE CASCADE,

    CONSTRAINT fk_notifications_meeting
        FOREIGN KEY (related_meeting_id)
        REFERENCES meetings(meeting_id)
        ON DELETE SET NULL,

    CONSTRAINT fk_notifications_group
        FOREIGN KEY (related_group_id)
        REFERENCES groups(group_id)
        ON DELETE SET NULL
);

-- =====================================================
-- TABLE: meeting_artifacts
-- Résumés / notes / transcriptions
-- =====================================================
CREATE TABLE IF NOT EXISTS meeting_artifacts (
    artifact_id UUID PRIMARY KEY,
    meeting_id UUID NOT NULL,
    artifact_type VARCHAR(50) NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT fk_meeting_artifacts_meeting
        FOREIGN KEY (meeting_id)
        REFERENCES meetings(meeting_id)
        ON DELETE CASCADE,

    CONSTRAINT chk_meeting_artifact_type
        CHECK (artifact_type IN ('summary', 'transcript', 'notes'))
);