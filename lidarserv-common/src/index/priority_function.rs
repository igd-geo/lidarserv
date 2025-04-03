use std::{
    cmp::{Ordering, max},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::geometry::grid::LeveledGridCell;

use super::writer::InsertionTask;

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum TaskPriorityFunction {
    NrPoints,
    Lod,
    OldestPoint,
    NewestPoint,
    TaskAge,
    NrPointsWeightedByTaskAge,
    NrPointsWeightedByOldestPoint,
    NrPointsWeightedByNegNewestPoint,

    /// (semi-)Private. Used during task cleanup.
    Cleanup,
}

#[derive(Debug, Copy, Clone, Error)]
#[error(
    "Invalid task priority function. Must be one of: 'NrPoints', 'Lod', 'OldestPoint', 'TaskAge', 'NrPointsTaskAge'"
)]
pub struct TaskPriorityFunctionFromStrErr;

impl FromStr for TaskPriorityFunction {
    type Err = TaskPriorityFunctionFromStrErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "NrPoints" => Ok(TaskPriorityFunction::NrPoints),
            "Lod" => Ok(TaskPriorityFunction::Lod),
            "OldestPoint" => Ok(TaskPriorityFunction::OldestPoint),
            "TaskAge" => Ok(TaskPriorityFunction::TaskAge),
            "NrPointsTaskAge" => Ok(TaskPriorityFunction::NrPointsWeightedByTaskAge),
            _ => Err(TaskPriorityFunctionFromStrErr),
        }
    }
}

impl std::fmt::Display for TaskPriorityFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            TaskPriorityFunction::NrPoints => "NrPoints",
            TaskPriorityFunction::Lod => "Lod",
            TaskPriorityFunction::OldestPoint => "OldestPoint",
            TaskPriorityFunction::NewestPoint => "NewestPoint",
            TaskPriorityFunction::TaskAge => "TaskAge",
            TaskPriorityFunction::NrPointsWeightedByTaskAge => "NrPointsTaskAge",
            TaskPriorityFunction::NrPointsWeightedByOldestPoint => "NrPointsOldestPoint",
            TaskPriorityFunction::NrPointsWeightedByNegNewestPoint => "NrPointsNegNewestPoint",
            TaskPriorityFunction::Cleanup => "LodInverse",
        };
        str.fmt(f)
    }
}

impl TaskPriorityFunction {
    pub(super) fn cmp(
        &self,
        cell_1: &LeveledGridCell,
        task_1: &InsertionTask,
        cell_2: &LeveledGridCell,
        task_2: &InsertionTask,
    ) -> Ordering {
        match self {
            TaskPriorityFunction::NrPoints => task_1.points.len().cmp(&task_2.points.len()),
            TaskPriorityFunction::Lod => (cell_1.lod, u32::MAX - task_1.created_generation)
                .cmp(&(cell_2.lod, u32::MAX - task_2.created_generation)),
            TaskPriorityFunction::Cleanup => cell_1.lod.cmp(&cell_2.lod).reverse(),
            TaskPriorityFunction::OldestPoint => task_2.min_generation.cmp(&task_1.min_generation),
            TaskPriorityFunction::NewestPoint => task_2.max_generation.cmp(&task_1.max_generation),
            TaskPriorityFunction::TaskAge => {
                task_2.created_generation.cmp(&task_1.created_generation)
            }
            TaskPriorityFunction::NrPointsWeightedByTaskAge => {
                let base = max(task_1.created_generation, task_2.created_generation);
                let l = task_1.points.len() as f64
                    * 2.0_f64.powi((base - task_1.created_generation) as i32);
                let r = task_2.points.len() as f64
                    * 2.0_f64.powi((base - task_2.created_generation) as i32);
                l.partial_cmp(&r).unwrap_or_else(|| unreachable!())
            }
            TaskPriorityFunction::NrPointsWeightedByOldestPoint => {
                let base = max(task_1.min_generation, task_2.min_generation);
                let l = task_1.points.len() as f64
                    * 2.0_f64.powi((base - task_1.min_generation) as i32);
                let r = task_2.points.len() as f64
                    * 2.0_f64.powi((base - task_2.min_generation) as i32);
                l.partial_cmp(&r).unwrap_or_else(|| unreachable!())
            }
            TaskPriorityFunction::NrPointsWeightedByNegNewestPoint => {
                let base = max(task_1.max_generation, task_2.max_generation);
                let l = task_1.points.len() as f64
                    * 2.0_f64.powi((base - task_1.max_generation) as i32);
                let r = task_2.points.len() as f64
                    * 2.0_f64.powi((base - task_2.max_generation) as i32);
                l.partial_cmp(&r).unwrap_or_else(|| unreachable!())
            }
        }
    }
}
