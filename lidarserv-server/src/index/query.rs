use std::str::FromStr;

use lidarserv_common::{
    geometry::{bounding_box::Aabb, grid::LodLevel},
    query::{
        aabb::AabbQuery, and::AndQuery, empty::EmptyQuery, full::FullQuery, lod::LodQuery,
        not::NotQuery, or::OrQuery, view_frustum::ViewFrustumQuery, ExecutableQuery,
        Query as QueryTrait,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Query {
    Empty,
    Full,
    Not(Box<Query>),
    And(Vec<Query>),
    Or(Vec<Query>),
    Aabb(Aabb<f64>),
    Lod(LodLevel),
    ViewFrustum(ViewFrustumQuery),
}

impl QueryTrait for Query {
    type Executable = Box<dyn ExecutableQuery>;

    fn prepare(self, ctx: &lidarserv_common::query::QueryContext) -> Self::Executable {
        match self {
            Query::Empty => Box::new(EmptyQuery.prepare(ctx)),
            Query::Full => Box::new(FullQuery.prepare(ctx)),
            Query::Not(inner) => Box::new(NotQuery(*inner).prepare(ctx)),
            Query::And(inner) => Box::new(AndQuery(inner).prepare(ctx)),
            Query::Or(inner) => Box::new(OrQuery(inner).prepare(ctx)),
            Query::Aabb(bounds) => Box::new(AabbQuery(bounds).prepare(ctx)),
            Query::ViewFrustum(q) => Box::new(q.prepare(ctx)),
            Query::Lod(lod) => Box::new(LodQuery(lod).prepare(ctx)),
        }
    }
}

impl Query {
    pub fn parse(querystring: &str) -> anyhow::Result<Query> {
        query_language::parse(querystring)
    }
}

impl FromStr for Query {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Query::parse(s)
    }
}

mod query_language {
    use crate::index::query::Query;
    use anyhow::{anyhow, Result};
    use lidarserv_common::{
        geometry::{bounding_box::Aabb, grid::LodLevel},
        query::view_frustum::ViewFrustumQuery,
    };
    use nalgebra::{Point3, Vector2, Vector3};
    use pest::{iterators::Pair, Parser};
    use pest_derive::Parser;

    #[derive(Parser)]
    #[grammar = "index//query_grammar.pest"]
    struct QueryParser;

    pub(super) fn parse(input: &str) -> Result<Query> {
        let parsed = QueryParser::parse(Rule::full_query, input)?.next().unwrap();
        parse_query(parsed)
    }

    fn parse_query(pair: Pair<Rule>) -> Result<Query> {
        match pair.as_rule() {
            Rule::empty => Ok(Query::Empty),
            Rule::full => Ok(Query::Full),
            Rule::lod => parse_lod(pair),
            Rule::aabb => parse_aabb(pair),
            Rule::view_frustum => parse_view_frustum(pair),
            Rule::bracket => parse_bracket(pair),
            Rule::not => parse_not(pair),
            Rule::and => parse_and(pair),
            Rule::or => parse_or(pair),
            _ => unreachable!(),
        }
    }

    fn parse_number(pair: Pair<Rule>) -> Result<f64> {
        assert_eq!(pair.as_rule(), Rule::number);
        Ok(pair.as_str().parse::<f64>()?)
    }

    fn parse_integer_u8(pair: Pair<Rule>) -> Result<u8> {
        assert_eq!(pair.as_rule(), Rule::integer);
        Ok(pair.as_str().parse::<u8>()?)
    }

    fn parse_and(pair: Pair<Rule>) -> Result<Query> {
        assert_eq!(pair.as_rule(), Rule::and);
        let subqueries = pair
            .into_inner()
            .map(|p| parse_query(p))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Query::And(subqueries))
    }

    fn parse_or(pair: Pair<Rule>) -> Result<Query> {
        assert_eq!(pair.as_rule(), Rule::or);
        let subqueries = pair
            .into_inner()
            .map(|p| parse_query(p))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Query::Or(subqueries))
    }

    fn parse_not(pair: Pair<Rule>) -> Result<Query> {
        assert_eq!(pair.as_rule(), Rule::not);
        let inner = pair.into_inner().unwrap_1();
        let inner = parse_query(inner)?;
        Ok(Query::Not(Box::new(inner)))
    }

    fn parse_bracket(pair: Pair<Rule>) -> Result<Query> {
        assert_eq!(pair.as_rule(), Rule::bracket);
        let inner = pair.into_inner().unwrap_1();
        parse_query(inner)
    }

    fn parse_lod(pair: Pair<Rule>) -> Result<Query> {
        assert_eq!(pair.as_rule(), Rule::lod);
        let inner = pair.into_inner().unwrap_1();
        let level = parse_integer_u8(inner)?;
        Ok(Query::Lod(LodLevel::from_level(level)))
    }

    fn parse_aabb(pair: Pair<Rule>) -> Result<Query> {
        assert_eq!(pair.as_rule(), Rule::aabb);
        let (inner1, inner2) = pair.into_inner().map(parse_coordinate).unwrap_2();
        let mut aabb = Aabb::empty();
        aabb.extend(inner1?.into());
        aabb.extend(inner2?.into());
        Ok(Query::Aabb(aabb))
    }

    fn parse_coordinate(pair: Pair<Rule>) -> Result<Vector3<f64>> {
        assert_eq!(pair.as_rule(), Rule::coordinate);
        let (x, y, z) = pair.into_inner().map(parse_number).unwrap_3();
        Ok(Vector3::new(x?, y?, z?))
    }

    fn parse_coordinate2(pair: Pair<Rule>) -> Result<Vector2<f64>> {
        assert_eq!(pair.as_rule(), Rule::coordinate2);
        let (x, y) = pair.into_inner().map(parse_number).unwrap_2();
        Ok(Vector2::new(x?, y?))
    }

    fn parse_view_frustum(pair: Pair<Rule>) -> Result<Query> {
        assert_eq!(pair.as_rule(), Rule::view_frustum);
        let mut camera_pos = None;
        let mut camera_dir = None;
        let mut camera_up = None;
        let mut fov_y = None;
        let mut z_near = None;
        let mut z_far = None;
        let mut window_size = None;
        let mut max_distance = None;

        for p in pair.into_inner() {
            match p.as_node_tag() {
                Some("cp") => camera_pos = Some(Point3::from(parse_coordinate(p)?)),
                Some("cd") => camera_dir = Some(parse_coordinate(p)?),
                Some("cu") => camera_up = Some(parse_coordinate(p)?),
                Some("fov") => fov_y = Some(parse_number(p)?),
                Some("zn") => z_near = Some(parse_number(p)?),
                Some("zf") => z_far = Some(parse_number(p)?),
                Some("ws") => window_size = Some(parse_coordinate2(p)?),
                Some("md") => max_distance = Some(parse_number(p)?),
                _ => unreachable!(),
            }
        }

        let Some(camera_pos) = camera_pos else {
            return Err(anyhow!("view_frustum is missing parameter 'camera_pos'."));
        };
        let Some(camera_dir) = camera_dir else {
            return Err(anyhow!("view_frustum is missing parameter 'camera_dir'."));
        };
        let Some(camera_up) = camera_up else {
            return Err(anyhow!("view_frustum is missing parameter 'camera_up'."));
        };
        let Some(fov_y) = fov_y else {
            return Err(anyhow!("view_frustum is missing parameter 'fov_y'."));
        };
        let Some(z_near) = z_near else {
            return Err(anyhow!("view_frustum is missing parameter 'z_near'."));
        };
        let Some(z_far) = z_far else {
            return Err(anyhow!("view_frustum is missing parameter 'z_far'."));
        };
        let Some(window_size) = window_size else {
            return Err(anyhow!("view_frustum is missing parameter 'window_size'."));
        };
        let Some(max_distance) = max_distance else {
            return Err(anyhow!("view_frustum is missing parameter 'max_distance'."));
        };

        Ok(Query::ViewFrustum(ViewFrustumQuery {
            camera_pos,
            camera_dir,
            camera_up,
            fov_y,
            z_near,
            z_far,
            window_size,
            max_distance,
        }))
    }

    trait IteratorExt: Iterator + Sized {
        fn unwrap_next(&mut self) -> Self::Item {
            self.next().unwrap()
        }

        fn unwrap_done(mut self) {
            if self.next().is_some() {
                panic!()
            }
        }

        fn unwrap_1(mut self) -> Self::Item {
            let item = self.unwrap_next();
            self.unwrap_done();
            item
        }

        fn unwrap_2(mut self) -> (Self::Item, Self::Item) {
            let item1 = self.unwrap_next();
            let item2 = self.unwrap_next();
            self.unwrap_done();
            (item1, item2)
        }

        fn unwrap_3(mut self) -> (Self::Item, Self::Item, Self::Item) {
            let item1 = self.unwrap_next();
            let item2 = self.unwrap_next();
            let item3 = self.unwrap_next();
            self.unwrap_done();
            (item1, item2, item3)
        }
    }

    impl<T> IteratorExt for T where T: Iterator {}
}

#[cfg(test)]
mod test {
    use std::f64::consts::FRAC_PI_2;

    use super::Query;
    use lidarserv_common::{
        geometry::{bounding_box::Aabb, grid::LodLevel},
        query::view_frustum::ViewFrustumQuery,
    };
    use nalgebra::{point, vector};
    use serde_json::json;

    fn query_test_json(q: Query, expected_json: serde_json::Value) {
        let actual_json = serde_json::to_value(q).unwrap();
        assert_eq!(expected_json, actual_json)
    }

    fn query_test_parse(input: &str, query: Query) {
        let s = Query::parse(input).unwrap();
        assert_eq!(s, query);
    }

    /// Tests that the json representation of a query (as serialized by serde)
    /// is as intended.
    #[test]
    fn test_json() {
        query_test_json(Query::Empty, json!("Empty"));
        query_test_json(Query::Full, json!("Full"));
        query_test_json(
            Query::Not(Box::new(Query::Empty)),
            json!({
                "Not": "Empty"
            }),
        );
        query_test_json(
            Query::And(vec![]),
            json!({
                "And": []
            }),
        );
        query_test_json(
            Query::And(vec![Query::Full, Query::Empty]),
            json!({
                "And": ["Full", "Empty"]
            }),
        );
        query_test_json(
            Query::Or(vec![]),
            json!({
                "Or": []
            }),
        );
        query_test_json(
            Query::Or(vec![Query::Full, Query::Empty]),
            json!({
                "Or": ["Full", "Empty"]
            }),
        );
        query_test_json(
            Query::Aabb(Aabb::new(point![1.0, 2.0, 3.0], point![4.0, 5.0, 6.0])),
            json!({
                "Aabb": {
                    "min": [1.0, 2.0, 3.0],
                    "max": [4.0, 5.0, 6.0]
                }
            }),
        );
        query_test_json(
            Query::Lod(LodLevel::base()),
            json!({
                "Lod": 0
            }),
        );
        query_test_json(
            Query::Lod(LodLevel::from_level(5)),
            json!({
                "Lod": 5
            }),
        );
        query_test_json(
            Query::ViewFrustum(ViewFrustumQuery {
                camera_pos: point![10.0, 11.0, 20.0],
                camera_dir: vector![1.0, 0.0, 0.0],
                camera_up: vector![0.0, 1.0, 0.0],
                fov_y: FRAC_PI_2,
                z_near: 0.1,
                z_far: 100.0,
                window_size: vector![1920.0, 1080.0],
                max_distance: 5.0,
            }),
            json!({
                "ViewFrustum": {
                    "camera_pos": [10.0, 11.0, 20.0],
                    "camera_dir": [1.0, 0.0, 0.0],
                    "camera_up": [0.0, 1.0, 0.0],
                    "fov_y": FRAC_PI_2,
                    "z_near": 0.1,
                    "z_far": 100.0,
                    "window_size": [1920.0, 1080.0],
                    "max_distance": 5.0,
                }
            }),
        )
    }

    #[test]
    fn test_query_language() {
        query_test_parse("full", Query::Full);
        query_test_parse("empty", Query::Empty);
        query_test_parse("lod(5)", Query::Lod(LodLevel::from_level(5)));
        query_test_parse(
            "aabb([1,2,3], [4,5,6])",
            Query::Aabb(Aabb::new(point![1.0, 2.0, 3.0], point![4.0, 5.0, 6.0])),
        );
        query_test_parse(
            "view_frustum(
                camera_pos: [1,2,3],
                camera_dir: [4,5,6],
                camera_up: [7,8,9],
                fov_y: 0.5,
                z_near: 0.1,
                z_far: 100,
                window_size: [300, 200] ,
                max_distance: 5
            )",
            Query::ViewFrustum(ViewFrustumQuery {
                camera_pos: point![1.0, 2.0, 3.0],
                camera_dir: vector![4.0, 5.0, 6.0],
                camera_up: vector![7.0, 8.0, 9.0],
                fov_y: 0.5,
                z_near: 0.1,
                z_far: 100.0,
                window_size: vector![300.0, 200.0],
                max_distance: 5.0,
            }),
        );
        query_test_parse("!full", Query::Not(Box::new(Query::Full)));
        query_test_parse("empty or full", Query::Or(vec![Query::Empty, Query::Full]));
        query_test_parse(
            "!empty or full",
            Query::Or(vec![Query::Not(Box::new(Query::Empty)), Query::Full]),
        );
        query_test_parse(
            "empty and full",
            Query::And(vec![Query::Empty, Query::Full]),
        );
        query_test_parse(
            "!empty and full",
            Query::And(vec![Query::Not(Box::new(Query::Empty)), Query::Full]),
        );
        query_test_parse(
            "lod(1) or lod(2) or lod(3)",
            Query::Or(vec![
                Query::Lod(LodLevel::from_level(1)),
                Query::Lod(LodLevel::from_level(2)),
                Query::Lod(LodLevel::from_level(3)),
            ]),
        );
        query_test_parse(
            "lod(1) and lod(2) or lod(3)",
            Query::Or(vec![
                Query::And(vec![
                    Query::Lod(LodLevel::from_level(1)),
                    Query::Lod(LodLevel::from_level(2)),
                ]),
                Query::Lod(LodLevel::from_level(3)),
            ]),
        );
        query_test_parse(
            "lod(1) or lod(2) and lod(3)",
            Query::Or(vec![
                Query::Lod(LodLevel::from_level(1)),
                Query::And(vec![
                    Query::Lod(LodLevel::from_level(2)),
                    Query::Lod(LodLevel::from_level(3)),
                ]),
            ]),
        );
        query_test_parse(
            "lod(1) and lod(2) and lod(3)",
            Query::And(vec![
                Query::Lod(LodLevel::from_level(1)),
                Query::Lod(LodLevel::from_level(2)),
                Query::Lod(LodLevel::from_level(3)),
            ]),
        );
    }
}
