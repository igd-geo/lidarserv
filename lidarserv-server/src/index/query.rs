use std::{convert::Infallible, str::FromStr};

use lidarserv_common::{
    geometry::{bounding_box::Aabb, grid::LodLevel},
    query::{
        ExecutableQuery, Query as QueryTrait, QueryContext,
        aabb::AabbQuery,
        and::AndQuery,
        attribute::{AttributeQuery, AttriuteQueryError, FilterableAttributeType, TestFunction},
        empty::EmptyQuery,
        full::FullQuery,
        lod::LodQuery,
        not::NotQuery,
        or::OrQuery,
        view_frustum::ViewFrustumQuery,
    },
};
use nalgebra::{Vector3, Vector4, vector};
use pasture_core::layout::{PointAttributeDataType, PointAttributeDefinition};
use serde::{Deserialize, Serialize, de::Visitor};

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
    Attribute {
        attribute_name: String,
        test: TestFunction<AttributeValue>,
    },
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum AttributeValueScalar {
    Int(i128),
    Float(f64),
}

impl Serialize for AttributeValueScalar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            AttributeValueScalar::Int(i) => serializer.serialize_i128(*i),
            AttributeValueScalar::Float(f) => serializer.serialize_f64(*f),
        }
    }
}

impl<'de> Deserialize<'de> for AttributeValueScalar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ScalarVisitor;

        impl Visitor<'_> for ScalarVisitor {
            type Value = AttributeValueScalar;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "Scalar value")
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(AttributeValueScalar::Int(v as i128))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(AttributeValueScalar::Int(v as i128))
            }

            fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(AttributeValueScalar::Int(v))
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(AttributeValueScalar::Float(v))
            }
        }

        deserializer.deserialize_any(ScalarVisitor)
    }
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq)]
#[serde(untagged)]
pub enum AttributeValue {
    Scalar(AttributeValueScalar),
    Vec3(Vector3<AttributeValueScalar>),
    Vec4(Vector4<AttributeValueScalar>),
}

trait ConvertAttributeScalar: Sized {
    fn from_int(v: i128) -> Result<Self, &'static str>;
    fn from_float(v: f64) -> Result<Self, &'static str>;

    fn from_scalar_value(v: AttributeValueScalar) -> Result<Self, &'static str> {
        match v {
            AttributeValueScalar::Int(i) => Self::from_int(i),
            AttributeValueScalar::Float(f) => Self::from_float(f),
        }
    }
}
trait ConvertAttributeValue: Sized {
    fn from_scalar(v: AttributeValueScalar) -> Result<Self, &'static str>;
    fn from_vec3(v: Vector3<AttributeValueScalar>) -> Result<Self, &'static str>;
    fn from_vec4(v: Vector4<AttributeValueScalar>) -> Result<Self, &'static str>;

    fn from_value(v: AttributeValue) -> Result<Self, &'static str> {
        match v {
            AttributeValue::Scalar(s) => Self::from_scalar(s),
            AttributeValue::Vec3(vec3) => Self::from_vec3(vec3),
            AttributeValue::Vec4(vec4) => Self::from_vec4(vec4),
        }
    }
}

macro_rules! impl_convert_int {
    ($t:ty) => {
        impl ConvertAttributeScalar for $t {
            fn from_int(v: i128) -> Result<Self, &'static str> {
                if v >= <$t>::MIN as i128 && v <= <$t>::MAX as i128 {
                    Ok(v as $t)
                } else {
                    Err("attribute value is out of range.")
                }
            }

            fn from_float(_: f64) -> Result<Self, &'static str> {
                Err("expected integer, found float.")
            }
        }

        impl ConvertAttributeValue for $t {
            fn from_scalar(v: AttributeValueScalar) -> Result<Self, &'static str> {
                <$t>::from_scalar_value(v)
            }

            fn from_vec3(_: Vector3<AttributeValueScalar>) -> Result<Self, &'static str> {
                Err("expected a scalar value, found vector.")
            }

            fn from_vec4(_: Vector4<AttributeValueScalar>) -> Result<Self, &'static str> {
                Err("expected a scalar value, found vector.")
            }
        }
    };
}

impl_convert_int!(u8);
impl_convert_int!(u16);
impl_convert_int!(u32);
impl_convert_int!(u64);
impl_convert_int!(i8);
impl_convert_int!(i16);
impl_convert_int!(i32);
impl_convert_int!(i64);

macro_rules! impl_convert_float {
    ($t:ty) => {
        impl ConvertAttributeScalar for $t {
            fn from_int(v: i128) -> Result<Self, &'static str> {
                Ok(v as $t)
            }

            fn from_float(v: f64) -> Result<Self, &'static str> {
                Ok(v as $t)
            }
        }
        impl ConvertAttributeValue for $t {
            fn from_scalar(v: AttributeValueScalar) -> Result<Self, &'static str> {
                <$t>::from_scalar_value(v)
            }

            fn from_vec3(_: Vector3<AttributeValueScalar>) -> Result<Self, &'static str> {
                Err("expected a scalar value, found vector.")
            }

            fn from_vec4(_: Vector4<AttributeValueScalar>) -> Result<Self, &'static str> {
                Err("expected a scalar value, found vector.")
            }
        }
    };
}

impl_convert_float!(f32);
impl_convert_float!(f64);

impl<T> ConvertAttributeValue for Vector3<T>
where
    T: ConvertAttributeScalar,
{
    fn from_scalar(_: AttributeValueScalar) -> Result<Self, &'static str> {
        Err("expected a vector, found scalar value.")
    }

    fn from_vec3(v: Vector3<AttributeValueScalar>) -> Result<Self, &'static str> {
        Ok(vector![
            T::from_scalar_value(v[0])?,
            T::from_scalar_value(v[1])?,
            T::from_scalar_value(v[2])?,
        ])
    }

    fn from_vec4(_: Vector4<AttributeValueScalar>) -> Result<Self, &'static str> {
        Err("expected a 3d vector, found 4d vector.")
    }
}

impl<T> ConvertAttributeValue for Vector4<T>
where
    T: ConvertAttributeScalar,
{
    fn from_scalar(_: AttributeValueScalar) -> Result<Self, &'static str> {
        Err("expected a vector, found scalar value.")
    }

    fn from_vec3(_: Vector3<AttributeValueScalar>) -> Result<Self, &'static str> {
        Err("expected a 4d vector, found 3d vector.")
    }

    fn from_vec4(v: Vector4<AttributeValueScalar>) -> Result<Self, &'static str> {
        Ok(vector![
            T::from_scalar_value(v[0])?,
            T::from_scalar_value(v[1])?,
            T::from_scalar_value(v[2])?,
            T::from_scalar_value(v[3])?,
        ])
    }
}

#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error)]
pub enum QueryError {
    #[error(
        "The attribute {0} does not exist in this point cloud. (Attribute names are case sensitive.)"
    )]
    AttributeNotFound(String),

    #[error("Invalid type for attribute {0}: {1}")]
    TypeError(PointAttributeDefinition, &'static str),
}

impl From<Infallible> for QueryError {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

impl QueryTrait for Query {
    type Executable = Box<dyn ExecutableQuery>;
    type Error = QueryError;

    fn prepare(
        self,
        ctx: &lidarserv_common::query::QueryContext,
    ) -> Result<Self::Executable, Self::Error> {
        let prepared: Box<dyn ExecutableQuery> = match self {
            Query::Empty => Box::new(EmptyQuery.prepare(ctx)?),
            Query::Full => Box::new(FullQuery.prepare(ctx)?),
            Query::Not(inner) => Box::new(NotQuery(*inner).prepare(ctx)?),
            Query::And(inner) => Box::new(AndQuery(inner).prepare(ctx)?),
            Query::Or(inner) => Box::new(OrQuery(inner).prepare(ctx)?),
            Query::Aabb(bounds) => Box::new(AabbQuery(bounds).prepare(ctx)?),
            Query::ViewFrustum(q) => Box::new(q.prepare(ctx)?),
            Query::Lod(lod) => Box::new(LodQuery(lod).prepare(ctx)?),
            Query::Attribute {
                attribute_name,
                test,
            } => {
                let attr = match ctx.point_layout.get_attribute_by_name(&attribute_name) {
                    Some(a) => a.attribute_definition().clone(),
                    None => return Err(QueryError::AttributeNotFound(attribute_name)),
                };
                match attr.datatype() {
                    PointAttributeDataType::U8 => prepare_attribute_query::<u8>(attr, test, ctx)?,
                    PointAttributeDataType::I8 => prepare_attribute_query::<i8>(attr, test, ctx)?,
                    PointAttributeDataType::U16 => prepare_attribute_query::<u16>(attr, test, ctx)?,
                    PointAttributeDataType::I16 => prepare_attribute_query::<i16>(attr, test, ctx)?,
                    PointAttributeDataType::U32 => prepare_attribute_query::<u32>(attr, test, ctx)?,
                    PointAttributeDataType::I32 => prepare_attribute_query::<i32>(attr, test, ctx)?,
                    PointAttributeDataType::U64 => prepare_attribute_query::<u64>(attr, test, ctx)?,
                    PointAttributeDataType::I64 => prepare_attribute_query::<i64>(attr, test, ctx)?,
                    PointAttributeDataType::F32 => prepare_attribute_query::<f32>(attr, test, ctx)?,
                    PointAttributeDataType::F64 => prepare_attribute_query::<f64>(attr, test, ctx)?,
                    PointAttributeDataType::Vec3u8 => {
                        prepare_attribute_query::<Vector3<u8>>(attr, test, ctx)?
                    }
                    PointAttributeDataType::Vec3u16 => {
                        prepare_attribute_query::<Vector3<u16>>(attr, test, ctx)?
                    }
                    PointAttributeDataType::Vec3f32 => {
                        prepare_attribute_query::<Vector3<f32>>(attr, test, ctx)?
                    }
                    PointAttributeDataType::Vec3i32 => {
                        prepare_attribute_query::<Vector3<i32>>(attr, test, ctx)?
                    }
                    PointAttributeDataType::Vec3f64 => {
                        prepare_attribute_query::<Vector3<f64>>(attr, test, ctx)?
                    }
                    PointAttributeDataType::Vec4u8 => {
                        prepare_attribute_query::<Vector4<u8>>(attr, test, ctx)?
                    }
                    PointAttributeDataType::ByteArray(_) => {
                        return Err(QueryError::TypeError(
                            attr,
                            "byte array attributes can't be queried",
                        ));
                    }
                    PointAttributeDataType::Custom { .. } => {
                        return Err(QueryError::TypeError(
                            attr,
                            "custom datatypes can't be queried",
                        ));
                    }
                }
            }
        };
        Ok(prepared)
    }
}

fn prepare_attribute_query<T>(
    attr: PointAttributeDefinition,
    test: TestFunction<AttributeValue>,
    ctx: &QueryContext,
) -> Result<Box<dyn ExecutableQuery>, QueryError>
where
    T: ConvertAttributeValue + FilterableAttributeType,
{
    let test = test
        .map(|a| T::from_value(*a))
        .result()
        .map_err(|e| QueryError::TypeError(attr.clone(), e))?;
    let query = AttributeQuery {
        attribute: attr,
        test,
    };
    let prepared = match query.prepare(ctx) {
        Ok(o) => o,
        Err(AttriuteQueryError::AttributeType) => unreachable!(),
    };

    Ok(Box::new(prepared))
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
    use anyhow::{Result, anyhow};
    use lidarserv_common::{
        geometry::{bounding_box::Aabb, grid::LodLevel},
        query::{attribute::TestFunction, view_frustum::ViewFrustumQuery},
    };
    use nalgebra::{Point3, Vector2, Vector3, Vector4};
    use pest::{Parser, iterators::Pair};
    use pest_derive::Parser;

    use super::{AttributeValue, AttributeValueScalar};

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
            Rule::attribute_query => parse_attribute_query(pair),
            _ => unreachable!(),
        }
    }

    fn parse_number(pair: Pair<Rule>) -> Result<f64> {
        assert_eq!(pair.as_rule(), Rule::number);
        Ok(pair.as_str().parse::<f64>()?)
    }

    fn parse_positive_integer_u8(pair: Pair<Rule>) -> Result<u8> {
        assert_eq!(pair.as_rule(), Rule::pos_integer);
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
        let level = parse_positive_integer_u8(inner)?;
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

    fn parse_attribute_query(pair: Pair<Rule>) -> Result<Query> {
        assert_eq!(pair.as_rule(), Rule::attribute_query);
        let inner = pair.into_inner().unwrap_1();
        match inner.as_rule() {
            Rule::cmp_eq => {
                let cmp = parse_cmp2(inner)?;
                Ok(Query::Attribute {
                    attribute_name: cmp.attribute_name,
                    test: TestFunction::Eq(cmp.operand),
                })
            }
            Rule::cmp_lt => {
                let cmp = parse_cmp2(inner)?;
                Ok(Query::Attribute {
                    attribute_name: cmp.attribute_name,
                    test: TestFunction::Less(cmp.operand),
                })
            }
            Rule::cmp_le => {
                let cmp = parse_cmp2(inner)?;
                Ok(Query::Attribute {
                    attribute_name: cmp.attribute_name,
                    test: TestFunction::LessEq(cmp.operand),
                })
            }
            Rule::cmp_gt => {
                let cmp = parse_cmp2(inner)?;
                Ok(Query::Attribute {
                    attribute_name: cmp.attribute_name,
                    test: TestFunction::Greater(cmp.operand),
                })
            }
            Rule::cmp_ge => {
                let cmp = parse_cmp2(inner)?;
                Ok(Query::Attribute {
                    attribute_name: cmp.attribute_name,
                    test: TestFunction::GreaterEq(cmp.operand),
                })
            }
            Rule::cmp_ne => {
                let cmp = parse_cmp2(inner)?;
                Ok(Query::Attribute {
                    attribute_name: cmp.attribute_name,
                    test: TestFunction::Neq(cmp.operand),
                })
            }
            Rule::cmp_range_excl => {
                let cmp = parse_cmp3(inner)?;
                Ok(Query::Attribute {
                    attribute_name: cmp.attribute_name,
                    test: TestFunction::RangeExclusive(cmp.operand_l, cmp.operand_r),
                })
            }
            Rule::cmp_range_lincl => {
                let cmp = parse_cmp3(inner)?;
                Ok(Query::Attribute {
                    attribute_name: cmp.attribute_name,
                    test: TestFunction::RangeLeftInclusive(cmp.operand_l, cmp.operand_r),
                })
            }
            Rule::cmp_range_rincl => {
                let cmp = parse_cmp3(inner)?;
                Ok(Query::Attribute {
                    attribute_name: cmp.attribute_name,
                    test: TestFunction::RangeRightInclusive(cmp.operand_l, cmp.operand_r),
                })
            }
            Rule::cmp_range_incl => {
                let cmp = parse_cmp3(inner)?;
                Ok(Query::Attribute {
                    attribute_name: cmp.attribute_name,
                    test: TestFunction::RangeAllInclusive(cmp.operand_l, cmp.operand_r),
                })
            }
            _ => unreachable!(),
        }
    }

    struct Cmp2 {
        attribute_name: String,
        operand: AttributeValue,
    }

    struct Cmp3 {
        operand_l: AttributeValue,
        attribute_name: String,
        operand_r: AttributeValue,
    }

    fn parse_attribute_value(pair: Pair<Rule>) -> Result<AttributeValue> {
        match pair.as_rule() {
            Rule::coordinate3 => Ok(AttributeValue::Vec3(parse_attr_value_vec3(pair)?)),
            Rule::coordinate4 => Ok(AttributeValue::Vec4(parse_attr_value_vec4(pair)?)),
            Rule::number => Ok(AttributeValue::Scalar(parse_attr_value_scalar(pair)?)),
            _ => unreachable!(),
        }
    }

    fn parse_cmp2(pair: Pair<Rule>) -> Result<Cmp2> {
        let (a, b) = pair.into_inner().unwrap_2();
        Ok(Cmp2 {
            attribute_name: a.as_str().to_string(),
            operand: parse_attribute_value(b)?,
        })
    }

    fn parse_cmp3(pair: Pair<Rule>) -> Result<Cmp3> {
        let (a, b, c) = pair.into_inner().unwrap_3();
        Ok(Cmp3 {
            operand_l: parse_attribute_value(a)?,
            attribute_name: b.as_str().to_string(),
            operand_r: parse_attribute_value(c)?,
        })
    }

    fn parse_attr_value_scalar(pair: Pair<Rule>) -> Result<AttributeValueScalar> {
        let string = pair.as_str();
        if string.contains('.') {
            Ok(AttributeValueScalar::Float(string.parse::<f64>()?))
        } else {
            Ok(AttributeValueScalar::Int(string.parse::<i128>()?))
        }
    }

    fn parse_attr_value_vec3(pair: Pair<Rule>) -> Result<Vector3<AttributeValueScalar>> {
        assert_eq!(pair.as_rule(), Rule::coordinate3);
        let (x, y, z) = pair.into_inner().map(parse_attr_value_scalar).unwrap_3();
        Ok(Vector3::new(x?, y?, z?))
    }

    fn parse_attr_value_vec4(pair: Pair<Rule>) -> Result<Vector4<AttributeValueScalar>> {
        assert_eq!(pair.as_rule(), Rule::coordinate4);
        let (x, y, z, w) = pair.into_inner().map(parse_attr_value_scalar).unwrap_4();
        Ok(Vector4::new(x?, y?, z?, w?))
    }

    fn parse_coordinate(pair: Pair<Rule>) -> Result<Vector3<f64>> {
        assert_eq!(pair.as_rule(), Rule::coordinate3);
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

        fn unwrap_4(mut self) -> (Self::Item, Self::Item, Self::Item, Self::Item) {
            let item1 = self.unwrap_next();
            let item2 = self.unwrap_next();
            let item3 = self.unwrap_next();
            let item4 = self.unwrap_next();
            self.unwrap_done();
            (item1, item2, item3, item4)
        }
    }

    impl<T> IteratorExt for T where T: Iterator {}
}

#[cfg(test)]
mod test {
    use std::f64::consts::FRAC_PI_2;

    use super::{AttributeValue, AttributeValueScalar, Query};
    use lidarserv_common::{
        geometry::{bounding_box::Aabb, grid::LodLevel},
        query::{attribute::TestFunction, view_frustum::ViewFrustumQuery},
    };
    use nalgebra::{Vector3, Vector4, point, vector};
    use serde_json::json;

    fn query_test_json(q: Query, expected_json: serde_json::Value) {
        let actual_json = serde_json::to_value(&q).unwrap();
        assert_eq!(expected_json, actual_json);
        let read_back = serde_json::from_value::<Query>(actual_json).unwrap();
        assert_eq!(q, read_back);
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
        );
        query_test_json(
            Query::Attribute {
                attribute_name: "intensity".to_string(),
                test: TestFunction::Eq(AttributeValue::Scalar(AttributeValueScalar::Int(55))),
            },
            json!({
                "Attribute": {
                    "attribute_name": "intensity",
                    "test": {
                        "Eq": 55
                    }
                }
            }),
        );
        query_test_json(
            Query::Attribute {
                attribute_name: "gpstime".to_string(),
                test: TestFunction::Eq(AttributeValue::Scalar(AttributeValueScalar::Float(3.1))),
            },
            json!({
                "Attribute": {
                    "attribute_name": "gpstime",
                    "test": {
                        "Eq": 3.1
                    }
                }
            }),
        );
        query_test_json(
            Query::Attribute {
                attribute_name: "gpstime".to_string(),
                test: TestFunction::Eq(AttributeValue::Scalar(AttributeValueScalar::Float(3.0))),
            },
            json!({
                "Attribute": {
                    "attribute_name": "gpstime",
                    "test": {
                        "Eq": 3.0
                    }
                }
            }),
        );
        query_test_json(
            Query::Attribute {
                attribute_name: "normal".to_string(),
                test: TestFunction::Eq(AttributeValue::Vec3(Vector3::new(
                    AttributeValueScalar::Float(3.5),
                    AttributeValueScalar::Int(4),
                    AttributeValueScalar::Float(5.0),
                ))),
            },
            json!({
                "Attribute": {
                    "attribute_name": "normal",
                    "test": {
                        "Eq": [3.5, 4, 5.0]
                    }
                }
            }),
        );
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
        query_test_parse(
            "attr(classification == 4)",
            Query::Attribute {
                attribute_name: "classification".to_string(),
                test: TestFunction::Eq(AttributeValue::Scalar(AttributeValueScalar::Int(4))),
            },
        );
        query_test_parse(
            "attr(classification != 4)",
            Query::Attribute {
                attribute_name: "classification".to_string(),
                test: TestFunction::Neq(AttributeValue::Scalar(AttributeValueScalar::Int(4))),
            },
        );
        query_test_parse(
            "attr(intensity < 120)",
            Query::Attribute {
                attribute_name: "intensity".to_string(),
                test: TestFunction::Less(AttributeValue::Scalar(AttributeValueScalar::Int(120))),
            },
        );
        query_test_parse(
            "attr(intensity <= 120)",
            Query::Attribute {
                attribute_name: "intensity".to_string(),
                test: TestFunction::LessEq(AttributeValue::Scalar(AttributeValueScalar::Int(120))),
            },
        );
        query_test_parse(
            "attr(gpstime > 31.4)",
            Query::Attribute {
                attribute_name: "gpstime".to_string(),
                test: TestFunction::Greater(AttributeValue::Scalar(AttributeValueScalar::Float(
                    31.4,
                ))),
            },
        );
        query_test_parse(
            "attr(gpstime >= 31.4)",
            Query::Attribute {
                attribute_name: "gpstime".to_string(),
                test: TestFunction::GreaterEq(AttributeValue::Scalar(AttributeValueScalar::Float(
                    31.4,
                ))),
            },
        );
        query_test_parse(
            "attr(normal <= [0.1, 0.2, 5])",
            Query::Attribute {
                attribute_name: "normal".to_string(),
                test: TestFunction::LessEq(AttributeValue::Vec3(Vector3::new(
                    AttributeValueScalar::Float(0.1),
                    AttributeValueScalar::Float(0.2),
                    AttributeValueScalar::Int(5),
                ))),
            },
        );
        query_test_parse(
            "attr(foo <= [0.1, 0.2, 5, 6])",
            Query::Attribute {
                attribute_name: "foo".to_string(),
                test: TestFunction::LessEq(AttributeValue::Vec4(Vector4::new(
                    AttributeValueScalar::Float(0.1),
                    AttributeValueScalar::Float(0.2),
                    AttributeValueScalar::Int(5),
                    AttributeValueScalar::Int(6),
                ))),
            },
        );
        query_test_parse(
            "attr(negative < -10.0)",
            Query::Attribute {
                attribute_name: "negative".to_string(),
                test: TestFunction::Less(AttributeValue::Scalar(AttributeValueScalar::Float(
                    -10.0,
                ))),
            },
        );
        query_test_parse(
            "attr(negative < -10)",
            Query::Attribute {
                attribute_name: "negative".to_string(),
                test: TestFunction::Less(AttributeValue::Scalar(AttributeValueScalar::Int(-10))),
            },
        );
    }
}
