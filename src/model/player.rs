use super::{All, Model};
use crate::{operation::Get, schema::players, Result};
use diesel::{
    expression::bound::Bound, insert_into, sql_types, ExpressionMethods, PgConnection, QueryDsl,
    QueryResult, RunQueryDsl,
};
use serde_derive::Serialize;

mod get;
mod paginate;

pub use self::paginate::PlayerPagination;

#[derive(Queryable, Debug, Identifiable, Hash, Eq, PartialEq, Serialize)]
#[table_name = "players"]
pub struct Player {
    pub id: i32,
    pub name: String,
    pub banned: bool,
}

#[derive(Insertable, Debug)]
#[table_name = "players"]
struct NewPlayer<'a> {
    name: &'a str,
}

type WithName<'a> = diesel::dsl::Eq<players::name, Bound<sql_types::Text, &'a str>>;
type ByName<'a> = diesel::dsl::Filter<All<Player>, WithName<'a>>;

type WithId = diesel::dsl::Eq<players::id, Bound<sql_types::Int4, i32>>;
type ById = diesel::dsl::Filter<All<Player>, WithId>;

impl Player {
    pub fn by_name(name: &str) -> ByName {
        Player::all().filter(players::name.eq(name))
    }

    pub fn by_id(id: i32) -> ById {
        Player::all().filter(players::id.eq(id))
    }

    pub fn insert(name: &str, conn: &PgConnection) -> QueryResult<Player> {
        insert_into(players::table)
            .values(&NewPlayer { name })
            .get_result(conn)
    }

    pub fn name_to_id(name: &str, connection: &PgConnection) -> Result<i32> {
        Ok(Player::get(name, connection)?.id)
    }
}

impl Model for Player {
    type From = players::table;
    type Selection =  (players::id, players::name, players::banned);

    fn from() -> Self::From {
        players::table
    }

    fn selection() -> Self::Selection {
        Self::Selection::default()
    }
}
