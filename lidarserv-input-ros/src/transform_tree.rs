use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use log::warn;
use nalgebra::Matrix4;

use crate::ros::Transform;

pub struct TransformTree {
    nodes: HashMap<String, TreeNode>,
}

#[derive(Debug)]
enum TreeNode {
    IsStatic(Transform),
    IsDynamic(VecDeque<Transform>),
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum LookupError {
    #[error("Frame was not found.")]
    NotFound,

    #[error("Frame has not arrived yet.")]
    Wait,
}

impl TransformTree {
    pub fn new() -> Self {
        TransformTree {
            nodes: HashMap::new(),
        }
    }

    /// Add a transform to the tree.
    pub fn add(&mut self, new_transform: Transform) {
        let key = new_transform.frame.clone();
        if new_transform.is_static {
            self.nodes.insert(key, TreeNode::IsStatic(new_transform));
        } else {
            // get node for this frame
            let node = self
                .nodes
                .entry(new_transform.frame.clone())
                .or_insert_with(|| TreeNode::IsDynamic(VecDeque::new()));

            // ensure it is a dynamic route and return the buffered transforms
            let queue = loop {
                match node {
                    TreeNode::IsDynamic(queue) => break queue,
                    _ => *node = TreeNode::IsDynamic(VecDeque::new()),
                }
            };

            // Check that messages arrived in ascending order.
            // Drop otherwise.
            if queue
                .back()
                .is_some_and(|b| b.time_stamp > new_transform.time_stamp)
            {
                warn!("Out-of-order tf message");
                return;
            }

            // add new item
            queue.push_back(new_transform);
        }
    }

    /// Gets a transform from a specific frame to its direct parent frame at a
    /// given time stamp. If there no transform exists at this exact time stamp,
    /// the ones before and after are interpolated.
    ///
    /// Errors:
    ///
    /// If the time stamp is further in the past than the amount of history stored in the
    /// buffer, LookupError::NotFound is returned.
    /// But if the time stamp is in the future of the newest transform in the buffer, then
    /// retrying later (after adding newer transforms) might allow the method to succeed. In
    /// this case, LookupError::Wait is returned.
    fn get_transform_at(
        &self,
        frame: &str,
        time_stamp: Duration,
    ) -> Result<Transform, LookupError> {
        // Get node. Otherwise request to wait for the first message
        let Some(node) = self.nodes.get(frame) else {
            return Err(LookupError::Wait);
        };

        match node {
            TreeNode::IsStatic(transform) => Ok(transform.clone()),
            TreeNode::IsDynamic(queue) => {
                // if the time stamp is older than the oldest element in the queue,
                // then it is too old.
                if queue
                    .front()
                    .is_some_and(|front| time_stamp < front.time_stamp)
                {
                    return Err(LookupError::NotFound);
                }

                // find the first element that is newer than the time stamp
                let Some((index2, transform2)) = queue
                    .iter()
                    .enumerate()
                    .find(|(_, t)| time_stamp <= t.time_stamp)
                else {
                    return Err(LookupError::Wait);
                };

                // from above we know that
                //  - queue[0].time_stamp <= time_stamp
                //  - queue[index2].time_stamp >= time_stamp
                // therefore, if index2==0, we must have queue[index2].time_stamp == time_stamp
                if index2 == 0 {
                    return Ok(transform2.clone());
                }

                // interpolate with the element before.
                let index1 = index2 - 1; // we know that index2 > 0, so this won't underflow.
                let transform1 = &queue[index1];
                let mut frac = (time_stamp.as_secs_f64() - transform1.time_stamp.as_secs_f64())
                    / (transform2.time_stamp.as_secs_f64() - transform1.time_stamp.as_secs_f64());
                if !frac.is_finite() {
                    frac = 0.0;
                }
                let result = transform1.interpolate(frac, transform2);

                // done
                Ok(result)
            }
        }
    }

    /// Removes buffered transforms from before the given time stamp.
    ///
    /// One transform with a time stamp before the given one is always kept,
    /// so that it is guaranteed that transforms at the given time stamp
    /// or later can be interpolated.
    pub fn cleanup_before(&mut self, time_stamp: Duration) {
        for node in self.nodes.values_mut() {
            if let TreeNode::IsDynamic(queue) = node {
                let index = queue
                    .iter()
                    .enumerate()
                    .find_map(|(index, transform)| {
                        if transform.time_stamp > time_stamp {
                            Some(index)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(queue.len());
                if index > 0 {
                    let nr_delete = index - 1;
                    for _ in 0..nr_delete {
                        queue.pop_front();
                    }
                }
            }
        }
    }

    /// Traverses the chain of transforms up to the root.
    fn chain(&self, frame: &str, time_stamp: Duration) -> (Vec<Transform>, LookupError) {
        let mut chain: Vec<Transform> = Vec::new();

        let max_chain_len = 100; // to avoid cycles
        for _ in 0..max_chain_len {
            let frame = chain
                .last()
                .map(|c| c.parent_frame.as_str())
                .unwrap_or(frame);
            match self.get_transform_at(frame, time_stamp) {
                Ok(t) => {
                    chain.push(t);
                }
                Err(e) => return (chain, e),
            }
        }

        // in case of a cycle just fail
        (vec![], LookupError::NotFound)
    }

    /// Calculates a transformation matrix from one frame into another at the given point in time.
    pub fn transform(
        &self,
        time_stamp: Duration,
        src_frame: &str,
        dst_frame: &str,
    ) -> Result<Matrix4<f64>, LookupError> {
        if src_frame == dst_frame {
            return Ok(Matrix4::identity());
        }

        let (mut src_chain, src_e) = self.chain(src_frame, time_stamp);
        let (mut dst_chain, dst_e) = self.chain(dst_frame, time_stamp);

        // check that src_chain and dst_chain are related
        // (there must be a chain to the same root frame)
        let src_root = src_chain
            .last()
            .map(|t| t.parent_frame.as_str())
            .unwrap_or(src_frame);
        let dst_root = dst_chain
            .last()
            .map(|t| t.parent_frame.as_str())
            .unwrap_or(dst_frame);
        if src_root != dst_root {
            // If one of them returned LookupError::Wait, then there is still a chance
            // to find a common root after waiting for more transform messages.
            // Otherwise, return LookupError::NotFound.
            if src_e == LookupError::NotFound && dst_e == LookupError::NotFound {
                return Err(LookupError::NotFound);
            } else {
                return Err(LookupError::Wait);
            }
        }

        // remove the common part in the chain to the root frame.
        loop {
            let Some(last_src) = src_chain.last() else {
                break;
            };
            let Some(last_dst) = dst_chain.last() else {
                break;
            };
            if last_src.frame != last_dst.frame {
                break;
            } else {
                src_chain.pop();
                dst_chain.pop();
            }
        }

        // combine transforms into one matrix
        let forward = src_chain.into_iter().rev().map(|t| t.matrix());
        let backward = dst_chain.into_iter().map(|t| t.inverse_matrix());
        let matrix_chain = Iterator::chain(backward, forward);
        let matrix = matrix_chain
            .reduce(|l, r| l * r)
            .unwrap_or_else(Matrix4::identity);

        Ok(matrix)
    }
}

#[cfg(test)]
mod test {
    use std::{f64::consts::PI, time::Duration};

    use nalgebra::{vector, Matrix4, UnitQuaternion, Vector3};

    use crate::{ros::Transform, transform_tree::LookupError};

    use super::TransformTree;

    #[test]
    fn test_transformtree_get() {
        // create tree
        let mut tree = TransformTree::new();

        // fill with test data
        tree.add(Transform {
            frame: "static_frame".to_string(),
            parent_frame: "world".to_string(),
            is_static: true,
            time_stamp: Duration::from_secs(10),
            translation: vector![0.0, 0.0, 1.0],
            rotation: UnitQuaternion::identity(),
        });
        tree.add(Transform {
            frame: "dynamic_frame".to_string(),
            parent_frame: "world".to_string(),
            is_static: false,
            time_stamp: Duration::from_secs(5),
            translation: vector![4.0, 0.0, 0.0],
            rotation: UnitQuaternion::identity(),
        });
        tree.add(Transform {
            frame: "dynamic_frame".to_string(),
            parent_frame: "world".to_string(),
            is_static: false,
            time_stamp: Duration::from_secs(6),
            translation: vector![6.0, 0.0, 0.0],
            rotation: UnitQuaternion::identity(),
        });
        tree.add(Transform {
            frame: "dynamic_frame".to_string(),
            parent_frame: "world".to_string(),
            is_static: false,
            time_stamp: Duration::from_secs(7),
            translation: vector![7.0, 0.0, 0.0],
            rotation: UnitQuaternion::identity(),
        });
        tree.add(Transform {
            frame: "dynamic_frame_single".to_string(),
            parent_frame: "world".to_string(),
            is_static: false,
            time_stamp: Duration::from_secs(10),
            translation: vector![0.0, 3.0, 0.0],
            rotation: UnitQuaternion::identity(),
        });

        // make immutable
        let tree = tree;

        // test unknown topic
        assert_eq!(
            tree.get_transform_at("unknown", Duration::from_secs(5)),
            Err(LookupError::Wait)
        );

        // test static topic
        assert_eq!(
            tree.get_transform_at("static_frame", Duration::from_secs(5),),
            Ok(Transform {
                frame: "static_frame".to_string(),
                parent_frame: "world".to_string(),
                is_static: true,
                time_stamp: Duration::from_secs(10),
                translation: vector![0.0, 0.0, 1.0],
                rotation: UnitQuaternion::identity(),
            })
        );

        // test dynamic topic
        assert_eq!(
            tree.get_transform_at("dynamic_frame", Duration::from_secs(4),),
            Err(LookupError::NotFound)
        );
        assert_eq!(
            tree.get_transform_at("dynamic_frame", Duration::from_secs(5),),
            Ok(Transform {
                frame: "dynamic_frame".to_string(),
                parent_frame: "world".to_string(),
                is_static: false,
                time_stamp: Duration::from_secs(5),
                translation: vector![4.0, 0.0, 0.0],
                rotation: UnitQuaternion::identity(),
            })
        );
        assert_eq!(
            tree.get_transform_at("dynamic_frame", Duration::from_secs_f64(5.5),),
            Ok(Transform {
                frame: "dynamic_frame".to_string(),
                parent_frame: "world".to_string(),
                is_static: false,
                time_stamp: Duration::from_secs_f64(5.5),
                translation: vector![5.0, 0.0, 0.0],
                rotation: UnitQuaternion::identity(),
            })
        );
        assert_eq!(
            tree.get_transform_at("dynamic_frame", Duration::from_secs_f64(6.0),),
            Ok(Transform {
                frame: "dynamic_frame".to_string(),
                parent_frame: "world".to_string(),
                is_static: false,
                time_stamp: Duration::from_secs_f64(6.0),
                translation: vector![6.0, 0.0, 0.0],
                rotation: UnitQuaternion::identity(),
            })
        );
        assert_eq!(
            tree.get_transform_at("dynamic_frame", Duration::from_secs_f64(6.5),),
            Ok(Transform {
                frame: "dynamic_frame".to_string(),
                parent_frame: "world".to_string(),
                is_static: false,
                time_stamp: Duration::from_secs_f64(6.5),
                translation: vector![6.5, 0.0, 0.0],
                rotation: UnitQuaternion::identity(),
            })
        );
        assert_eq!(
            tree.get_transform_at("dynamic_frame", Duration::from_secs(7),),
            Ok(Transform {
                frame: "dynamic_frame".to_string(),
                parent_frame: "world".to_string(),
                is_static: false,
                time_stamp: Duration::from_secs(7),
                translation: vector![7.0, 0.0, 0.0],
                rotation: UnitQuaternion::identity(),
            })
        );
        assert_eq!(
            tree.get_transform_at("dynamic_frame", Duration::from_secs(8),),
            Err(LookupError::Wait)
        );

        // if exacly one transform has arrived yet
        assert_eq!(
            tree.get_transform_at("dynamic_frame_single", Duration::from_secs(9),),
            Err(LookupError::NotFound)
        );
        assert_eq!(
            tree.get_transform_at("dynamic_frame_single", Duration::from_secs(10),),
            Ok(Transform {
                frame: "dynamic_frame_single".to_string(),
                parent_frame: "world".to_string(),
                is_static: false,
                time_stamp: Duration::from_secs(10),
                translation: vector![0.0, 3.0, 0.0],
                rotation: UnitQuaternion::identity(),
            })
        );
        assert_eq!(
            tree.get_transform_at("dynamic_frame_single", Duration::from_secs(11),),
            Err(LookupError::Wait)
        );
    }

    #[test]
    fn test_transformtree_cleanup() {
        // create tree
        let mut tree = TransformTree::new();

        // fill with test data
        tree.add(Transform {
            frame: "frame".to_string(),
            parent_frame: "world".to_string(),
            is_static: false,
            time_stamp: Duration::from_secs(5),
            translation: vector![4.0, 0.0, 0.0],
            rotation: UnitQuaternion::identity(),
        });
        tree.add(Transform {
            frame: "frame".to_string(),
            parent_frame: "world".to_string(),
            is_static: false,
            time_stamp: Duration::from_secs(6),
            translation: vector![6.0, 0.0, 0.0],
            rotation: UnitQuaternion::identity(),
        });
        tree.add(Transform {
            frame: "frame".to_string(),
            parent_frame: "world".to_string(),
            is_static: false,
            time_stamp: Duration::from_secs(7),
            translation: vector![7.0, 0.0, 0.0],
            rotation: UnitQuaternion::identity(),
        });

        tree.cleanup_before(Duration::from_secs_f64(5.0));
        assert!(tree
            .get_transform_at("frame", Duration::from_secs_f64(5.0))
            .is_ok());

        tree.cleanup_before(Duration::from_secs_f64(6.0));
        assert_eq!(
            tree.get_transform_at("frame", Duration::from_secs_f64(5.0)),
            Err(LookupError::NotFound)
        );
        assert!(tree
            .get_transform_at("frame", Duration::from_secs_f64(6.0))
            .is_ok());

        tree.cleanup_before(Duration::from_secs_f64(6.5));
        assert!(tree
            .get_transform_at("frame", Duration::from_secs_f64(6.5))
            .is_ok());
    }

    #[test]
    fn test_transformtree_transform() {
        // create tree
        let mut tree = TransformTree::new();

        // fill with test data
        /*

        a3  c3     b3
         \ /      /
         a2      b2
          \     /
          a1   b1
           \  /
           root

        */
        let a3_to_a2 = Transform {
            frame: "a3".to_string(),
            parent_frame: "a2".to_string(),
            is_static: true,
            time_stamp: Duration::ZERO,
            translation: vector![1.0, 2.0, 3.0],
            rotation: UnitQuaternion::from_axis_angle(&Vector3::x_axis(), PI * 0.5),
        };
        let a2_to_a1 = Transform {
            frame: "a2".to_string(),
            parent_frame: "a1".to_string(),
            is_static: true,
            time_stamp: Duration::ZERO,
            translation: vector![4.0, 5.0, 6.0],
            rotation: UnitQuaternion::from_axis_angle(&Vector3::y_axis(), PI * 0.5),
        };
        let a1_to_root = Transform {
            frame: "a1".to_string(),
            parent_frame: "root".to_string(),
            is_static: true,
            time_stamp: Duration::ZERO,
            translation: vector![7.0, 8.0, 9.0],
            rotation: UnitQuaternion::from_axis_angle(&Vector3::z_axis(), PI * 0.5),
        };
        let b3_to_b2 = Transform {
            frame: "b3".to_string(),
            parent_frame: "b2".to_string(),
            is_static: true,
            time_stamp: Duration::ZERO,
            translation: vector![8.0, 7.0, 6.0],
            rotation: UnitQuaternion::identity(),
        };
        let b2_to_b1 = Transform {
            frame: "b2".to_string(),
            parent_frame: "b1".to_string(),
            is_static: true,
            time_stamp: Duration::ZERO,
            translation: vector![5.0, 4.0, 3.0],
            rotation: UnitQuaternion::from_axis_angle(&Vector3::x_axis(), -PI * 0.5),
        };
        let b1_to_root = Transform {
            frame: "b1".to_string(),
            parent_frame: "root".to_string(),
            is_static: true,
            time_stamp: Duration::ZERO,
            translation: vector![2.0, 1.0, 0.0],
            rotation: UnitQuaternion::from_axis_angle(&Vector3::y_axis(), -PI * 0.5),
        };
        let c3_to_a2 = Transform {
            frame: "c3".to_string(),
            parent_frame: "a2".to_string(),
            is_static: true,
            time_stamp: Duration::ZERO,
            translation: vector![2.0, 3.0, 5.0],
            rotation: UnitQuaternion::from_axis_angle(&Vector3::z_axis(), -PI * 0.5),
        };
        tree.add(a3_to_a2.clone());
        tree.add(a2_to_a1.clone());
        tree.add(a1_to_root.clone());
        tree.add(b3_to_b2.clone());
        tree.add(b2_to_b1.clone());
        tree.add(b1_to_root.clone());
        tree.add(c3_to_a2.clone());

        // test identity
        assert_eq!(
            tree.transform(Duration::ZERO, "x", "x").unwrap(),
            Matrix4::identity()
        );

        // test simple child -> parent
        assert_eq!(
            tree.transform(Duration::ZERO, "a2", "a1").unwrap(),
            a2_to_a1.matrix()
        );

        // test simple parent -> child
        assert_eq!(
            tree.transform(Duration::ZERO, "a1", "a2").unwrap(),
            a2_to_a1.inverse_matrix()
        );

        // test leaf to root
        assert_eq!(
            tree.transform(Duration::ZERO, "a3", "root").unwrap(),
            a1_to_root.matrix() * a2_to_a1.matrix() * a3_to_a2.matrix()
        );

        // test root to leaf
        assert_eq!(
            tree.transform(Duration::ZERO, "root", "a3").unwrap(),
            a3_to_a2.inverse_matrix() * a2_to_a1.inverse_matrix() * a1_to_root.inverse_matrix()
        );

        // test leaf -> root -> other leaf
        assert_eq!(
            tree.transform(Duration::ZERO, "a3", "b3").unwrap(),
            b3_to_b2.inverse_matrix()
                * b2_to_b1.inverse_matrix()
                * b1_to_root.inverse_matrix()
                * a1_to_root.matrix()
                * a2_to_a1.matrix()
                * a3_to_a2.matrix()
        );

        // test leaf -> inner node -> other leaf
        assert_eq!(
            tree.transform(Duration::ZERO, "a3", "c3").unwrap(),
            c3_to_a2.inverse_matrix() * a3_to_a2.matrix()
        );
    }
}
