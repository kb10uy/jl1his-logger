use sqlx::{AssertSqlSafe, Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    domain::{
        band::{format_frequency_mhz, parse_frequency_hz},
        mode::Mode,
    },
    store::{Fault, StoreError, WriteTx, access::Principal, account::user, new_id, now},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrequencyInput {
    pub freq: String,
    pub mode: String,
    pub submode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteFrequency {
    pub id: String,
    pub freq: String,
    pub mode: String,
    pub submode: String,
    pub created_at: String,
}

impl FavoriteFrequency {
    fn from_row(row: &SqliteRow) -> Result<FavoriteFrequency, StoreError> {
        Ok(FavoriteFrequency {
            id: row.try_get("id")?,
            freq: row.try_get("freq")?,
            mode: row.try_get("mode")?,
            submode: row.try_get("submode")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

const SELECT: &str = "SELECT id, freq, mode, submode, created_at FROM frequency_favorites";

pub async fn list(conn: &mut SqliteConnection, principal: &Principal) -> Result<Vec<FavoriteFrequency>, StoreError> {
    let user = user::of_principal(conn, principal).await?;
    sqlx::query(AssertSqlSafe(format!(
        "{SELECT} WHERE user_id = ? ORDER BY freq_hz, mode, submode"
    )))
    .bind(&user.id)
    .fetch_all(conn)
    .await?
    .iter()
    .map(FavoriteFrequency::from_row)
    .collect()
}

pub async fn add(
    tx: &mut WriteTx,
    principal: &Principal,
    input: &FrequencyInput,
) -> Result<FavoriteFrequency, StoreError> {
    let user = user::of_principal(tx, principal).await?;
    let (freq, mode, submode) = normalize(input)?;
    let hz = i64::try_from(freq).map_err(|_| invalid())?;
    sqlx::query(
        "INSERT INTO frequency_favorites (id, user_id, freq, freq_hz, mode, submode, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT (user_id, freq_hz, mode, submode) DO NOTHING",
    )
    .bind(new_id())
    .bind(&user.id)
    .bind(format_frequency_mhz(freq))
    .bind(hz)
    .bind(mode)
    .bind(submode)
    .bind(now())
    .execute(&mut **tx)
    .await?;
    let row = sqlx::query(AssertSqlSafe(format!(
        "{SELECT} WHERE user_id = ? AND freq_hz = ? AND mode = ? AND submode = ?"
    )))
    .bind(&user.id)
    .bind(hz)
    .bind(mode)
    .bind(submode)
    .fetch_one(&mut **tx)
    .await?;
    FavoriteFrequency::from_row(&row)
}

pub async fn remove(tx: &mut WriteTx, principal: &Principal, id: &str) -> Result<(), StoreError> {
    let user = user::of_principal(tx, principal).await?;
    let removed = sqlx::query("DELETE FROM frequency_favorites WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(&user.id)
        .execute(&mut **tx)
        .await?;
    match removed.rows_affected() {
        0 => Err(StoreError::NotFound("favorite frequency")),
        _ => Ok(()),
    }
}

fn normalize(input: &FrequencyInput) -> Result<(u64, &'static str, &'static str), StoreError> {
    let hz = parse_frequency_hz(&input.freq)
        .filter(|hz| *hz > 0)
        .ok_or_else(invalid)?;
    let mode = Mode::named(&input.mode).ok_or_else(invalid)?;
    let submode = match input.submode.trim() {
        "" => "",
        submode => mode
            .submodes
            .iter()
            .find(|s| s.eq_ignore_ascii_case(submode))
            .copied()
            .ok_or_else(invalid)?,
    };
    Ok((hz, mode.name, submode))
}

fn invalid() -> StoreError {
    StoreError::Invalid(Fault::coded(
        "favorite_frequency_invalid",
        "a favorite needs a frequency in MHz and a known mode",
    ))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::store::testing::World;

    fn input(freq: &str, mode: &str, submode: &str) -> FrequencyInput {
        FrequencyInput {
            freq: freq.into(),
            mode: mode.into(),
            submode: submode.into(),
        }
    }

    async fn add_one(world: &World, principal: &Principal, input: &FrequencyInput) -> FavoriteFrequency {
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let added = add(&mut tx, principal, input).await.unwrap();
        tx.commit().await.unwrap();
        added
    }

    async fn listed(world: &World, principal: &Principal) -> Vec<(String, String, String)> {
        let mut conn = world.db.acquire().await.unwrap();
        list(&mut conn, principal)
            .await
            .unwrap()
            .into_iter()
            .map(|f| (f.freq, f.mode, f.submode))
            .collect()
    }

    #[tokio::test]
    async fn keeps_each_users_favorites_by_frequency() {
        let world = World::new().await;
        let operator = Principal::user(&world.operator.id);
        let owner = Principal::user(&world.owner.id);
        add_one(&world, &operator, &input("145.000", "fm", "")).await;
        add_one(&world, &operator, &input(" 7.1 ", "SSB", "lsb")).await;
        add_one(&world, &operator, &input("14.074", "FT8", "")).await;
        add_one(&world, &owner, &input("50.2", "SSB", "USB")).await;

        let own = |f: &str, m: &str, s: &str| (f.to_owned(), m.to_owned(), s.to_owned());
        assert_eq!(
            listed(&world, &operator).await,
            [own("7.1", "SSB", "LSB"), own("14.074", "FT8", ""), own("145", "FM", "")]
        );
        assert_eq!(listed(&world, &owner).await, [own("50.2", "SSB", "USB")]);
    }

    #[tokio::test]
    async fn adding_the_same_frequency_and_mode_returns_the_existing_favorite() {
        let world = World::new().await;
        let operator = Principal::user(&world.operator.id);
        let first = add_one(&world, &operator, &input("7.100", "SSB", "LSB")).await;
        let again = add_one(&world, &operator, &input("7.1000", "ssb", "LSB")).await;
        assert_eq!(again, first);
        let other = add_one(&world, &operator, &input("7.100", "SSB", "USB")).await;
        assert_ne!(other.id, first.id);
        assert_eq!(listed(&world, &operator).await.len(), 2);
    }

    #[rstest]
    #[case("", "SSB", "")]
    #[case("0", "SSB", "")]
    #[case("7,1", "SSB", "")]
    #[case("7.1", "VOICE", "")]
    #[case("7.1", "SSB", "FT8")]
    #[case("7.1", "FT8", "LSB")]
    #[tokio::test]
    async fn refuses_a_frequency_or_mode_that_is_not_valid(
        #[case] freq: &str,
        #[case] mode: &str,
        #[case] submode: &str,
    ) {
        let world = World::new().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let err = add(
            &mut tx,
            &Principal::user(&world.operator.id),
            &input(freq, mode, submode),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, StoreError::Invalid(ref f) if f.code == Some("favorite_frequency_invalid")));
    }

    #[tokio::test]
    async fn removes_only_the_callers_own_favorite() {
        let world = World::new().await;
        let operator = Principal::user(&world.operator.id);
        let owner = Principal::user(&world.owner.id);
        let favorite = add_one(&world, &operator, &input("433", "FM", "")).await;

        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            remove(&mut tx, &owner, &favorite.id).await,
            Err(StoreError::NotFound(_))
        ));
        remove(&mut tx, &operator, &favorite.id).await.unwrap();
        assert!(matches!(
            remove(&mut tx, &operator, &favorite.id).await,
            Err(StoreError::NotFound(_))
        ));
        tx.commit().await.unwrap();
        assert!(listed(&world, &operator).await.is_empty());
    }
}
