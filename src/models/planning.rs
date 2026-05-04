// Modèles de réponse pour le planning.
//
// Le planning final regroupera :
// - les meetings planifiés,
// - les tâches attribuées à l'utilisateur.
//
// Pour l'instant, on branche seulement les meetings réels.
// Le champ `tasks` reste prêt pour la future partie IA.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PlanningResponse {
    pub meetings: Vec<PlanningMeeting>,
    pub tasks: Vec<PlanningTask>,
}

#[derive(Debug, Serialize)]
pub struct PlanningMeeting {
    pub id: String,
    pub title: String,
    pub date: String,
    pub start: String,
    pub end: String,
    pub participants: Vec<String>,
    pub group_ids: Vec<String>,
    pub ia_enabled: bool,
    pub status: String,
    pub meeting_type: String,
}

#[derive(Debug, Serialize)]
pub struct PlanningTask {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub due_at: Option<String>,
    pub status: String,
}