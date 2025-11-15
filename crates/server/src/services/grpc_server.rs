use crate::db;
use anyhow::Result;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use tracing::{error, info};

// Include the generated protobuf code
pub mod emotions {
    tonic::include_proto!("emotions");
}

use emotions::emotion_service_server::{EmotionService as EmotionServiceTrait, EmotionServiceServer};
use emotions::*;

pub struct GrpcServer {
    us_db: Arc<Mutex<Connection>>,
    address: String,
}

impl GrpcServer {
    pub fn new(us_db: Arc<Mutex<Connection>>, address: String) -> Self {
        Self { us_db, address }
    }

    pub async fn run(self) -> Result<()> {
        let addr = self.address.parse()?;
        info!("Starting gRPC server on {}", self.address);

        let service = EmotionServiceImpl {
            us_db: self.us_db.clone(),
        };

        tonic::transport::Server::builder()
            .add_service(EmotionServiceServer::new(service))
            .serve(addr)
            .await?;

        Ok(())
    }
}

struct EmotionServiceImpl {
    us_db: Arc<Mutex<Connection>>,
}

#[tonic::async_trait]
impl EmotionServiceTrait for EmotionServiceImpl {
    async fn get_emotion(
        &self,
        request: Request<GetEmotionRequest>,
    ) -> Result<Response<Emotion>, Status> {
        let req = request.into_inner();
        let conn = self.us_db.lock().await;

        match db::get_emotion_by_message_id(&conn, req.message_id) {
            Ok(Some(emotion)) => Ok(Response::new(emotion_to_proto(emotion))),
            Ok(None) => Err(Status::not_found(format!(
                "Emotion not found for message_id: {}",
                req.message_id
            ))),
            Err(e) => {
                error!("Database error: {}", e);
                Err(Status::internal("Database error"))
            }
        }
    }

    async fn get_emotions(
        &self,
        request: Request<GetEmotionsRequest>,
    ) -> Result<Response<EmotionsResponse>, Status> {
        let req = request.into_inner();
        let conn = self.us_db.lock().await;

        let emotion_filter = req.emotion_filter.as_deref();
        let min_confidence = req.min_confidence;
        let limit = req.limit.map(|l| l as i32);
        let offset = req.offset.map(|o| o as i32);

        match db::list_emotions(&conn, emotion_filter, min_confidence, limit, offset) {
            Ok(emotions) => {
                let total_count = db::count_emotions(&conn).unwrap_or(0);
                Ok(Response::new(EmotionsResponse {
                    emotions: emotions.into_iter().map(emotion_to_proto).collect(),
                    total_count,
                }))
            }
            Err(e) => {
                error!("Database error: {}", e);
                Err(Status::internal("Database error"))
            }
        }
    }

    async fn list_all_emotions(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<EmotionsResponse>, Status> {
        let conn = self.us_db.lock().await;

        match db::list_emotions(&conn, None, None, None, None) {
            Ok(emotions) => {
                let total_count = emotions.len() as i32;
                Ok(Response::new(EmotionsResponse {
                    emotions: emotions.into_iter().map(emotion_to_proto).collect(),
                    total_count,
                }))
            }
            Err(e) => {
                error!("Database error: {}", e);
                Err(Status::internal("Database error"))
            }
        }
    }

    async fn update_emotion(
        &self,
        request: Request<UpdateEmotionRequest>,
    ) -> Result<Response<EmotionResponse>, Status> {
        let req = request.into_inner();
        let conn = self.us_db.lock().await;

        match db::update_emotion(&conn, req.message_id, &req.emotion, req.confidence) {
            Ok(_) => {
                // If notes are provided, add to training set
                if let Some(notes) = req.notes {
                    if let Ok(Some(msg)) = db::get_message(&conn, req.message_id) {
                        if let Some(text) = msg.text {
                            let _ = db::insert_training_sample(
                                &conn,
                                Some(req.message_id),
                                &text,
                                &req.emotion,
                            );
                        }
                    }
                }

                // Fetch the updated emotion
                match db::get_emotion_by_message_id(&conn, req.message_id) {
                    Ok(Some(emotion)) => Ok(Response::new(EmotionResponse {
                        success: true,
                        message: "Emotion updated successfully".to_string(),
                        emotion: Some(emotion_to_proto(emotion)),
                    })),
                    _ => Ok(Response::new(EmotionResponse {
                        success: true,
                        message: "Emotion updated successfully".to_string(),
                        emotion: None,
                    })),
                }
            }
            Err(e) => {
                error!("Database error: {}", e);
                Err(Status::internal("Database error"))
            }
        }
    }

    async fn batch_update_emotions(
        &self,
        request: Request<tonic::Streaming<UpdateEmotionRequest>>,
    ) -> Result<Response<EmotionResponse>, Status> {
        let mut stream = request.into_inner();
        let mut count = 0;

        while let Some(req) = stream.message().await? {
            let conn = self.us_db.lock().await;
            match db::update_emotion(&conn, req.message_id, &req.emotion, req.confidence) {
                Ok(_) => {
                    count += 1;
                    if let Some(notes) = req.notes {
                        if let Ok(Some(msg)) = db::get_message(&conn, req.message_id) {
                            if let Some(text) = msg.text {
                                let _ = db::insert_training_sample(
                                    &conn,
                                    Some(req.message_id),
                                    &text,
                                    &req.emotion,
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to update emotion for message {}: {}", req.message_id, e);
                }
            }
            drop(conn);
        }

        Ok(Response::new(EmotionResponse {
            success: true,
            message: format!("Updated {} emotions", count),
            emotion: None,
        }))
    }

    async fn delete_emotion(
        &self,
        request: Request<DeleteEmotionRequest>,
    ) -> Result<Response<EmotionResponse>, Status> {
        let req = request.into_inner();
        let conn = self.us_db.lock().await;

        match db::delete_emotion(&conn, req.id) {
            Ok(_) => Ok(Response::new(EmotionResponse {
                success: true,
                message: format!("Emotion {} deleted successfully", req.id),
                emotion: None,
            })),
            Err(e) => {
                error!("Database error: {}", e);
                Err(Status::internal("Database error"))
            }
        }
    }
}

fn emotion_to_proto(emotion: crate::models::Emotion) -> Emotion {
    Emotion {
        id: emotion.id,
        message_id: emotion.message_id,
        emotion: emotion.emotion,
        confidence: emotion.confidence,
        model_version: emotion.model_version,
        created_at: emotion.created_at.timestamp(),
        updated_at: emotion.updated_at.timestamp(),
    }
}
