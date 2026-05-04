use axum::{
    Router,
    routing::{delete, get, post},
};

use crate::{
    api::handlers,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        // Healthcheck simple.
        .route("/health", get(handlers::health))

        // Dashboard utilisateur connecté.
        .route("/dashboard", get(handlers::get_dashboard))

        // Planning utilisateur connecté.
        .route("/planning", get(handlers::get_planning))

        // Placeholder meetings.
        .route("/meetings", post(handlers::create_meeting))

        // Groupes.
        .route("/groups", get(handlers::list_groups).post(handlers::create_group))
        .route(
            "/groups/{group_id}",
            get(handlers::get_group_detail).delete(handlers::delete_group),
        )

        // Invitation d'un membre dans un groupe.
        // Remplace l'ancien ajout direct.
        .route("/groups/{group_id}/members", post(handlers::invite_group_member))

        // Retrait d'un membre déjà accepté.
        .route(
            "/groups/{group_id}/members/{user_keycloak_id}",
            delete(handlers::remove_group_member),
        )

        // Annulation d'une invitation pending par owner/admin.
        .route(
            "/groups/{group_id}/invitations/{group_invitation_id}",
            delete(handlers::cancel_group_invitation),
        )

        // Upload photo du groupe.
        .route("/groups/{group_id}/photo", post(handlers::upload_group_photo))

        // Recherche utilisateurs TrueGather.
        .route("/users/search", get(handlers::search_users))

        // Invitations de groupe reçues par l'utilisateur connecté.
        .route("/group-invitations/me", get(handlers::list_my_group_invitations))

        // Réponse à une invitation de groupe.
        // action = accept | decline
        .route(
            "/group-invitations/{group_invitation_id}/respond",
            post(handlers::respond_to_group_invitation),
        )

        // Notifications.
        .route(
            "/notifications/mark-all-read",
            post(handlers::mark_all_notifications_as_read),
        )
        .route(
            "/notifications/{notification_id}/read",
            post(handlers::mark_notification_as_read),
        )
}
