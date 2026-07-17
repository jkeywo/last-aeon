//! The campaign calendar.
//!
//! A 360-day year of twelve 30-day months. The setting canon prescribes no
//! civil calendar, so this invented convention optimises for the simulation:
//! equal-length months keep monthly rates exact in fixed-point arithmetic,
//! and a plain day count serialises compactly. One simulation tick is one
//! day; month and year boundaries trigger the slower strategic pulses.

use core::fmt;

use serde::{Deserialize, Serialize};

/// Days in every month.
pub const DAYS_PER_MONTH: i64 = 30;
/// Months in every year.
pub const MONTHS_PER_YEAR: i64 = 12;
/// Days in every year.
pub const DAYS_PER_YEAR: i64 = DAYS_PER_MONTH * MONTHS_PER_YEAR;

/// A campaign date as a day count from the calendar epoch.
///
/// Day zero is the first day of year zero. Scenarios choose their in-fiction
/// year numbering by picking a start date; nothing in the simulation assumes
/// campaigns begin at the epoch.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct GameDate(i64);

impl GameDate {
    /// The calendar epoch: year 0, month 1, day 1.
    pub const EPOCH: GameDate = GameDate(0);

    /// A date from a raw day count since the epoch.
    pub fn from_days(days: i64) -> Self {
        Self(days)
    }

    /// Days since the epoch (negative before it).
    pub fn days_since_epoch(self) -> i64 {
        self.0
    }

    /// This date shifted by a signed number of days.
    pub fn add_days(self, days: i64) -> Self {
        Self(self.0 + days)
    }

    /// The number of days from `self` to `other` (positive if `other` is later).
    pub fn days_until(self, other: GameDate) -> i64 {
        other.0 - self.0
    }

    /// The calendar form of this date.
    pub fn calendar(self) -> CalendarDate {
        let year = self.0.div_euclid(DAYS_PER_YEAR);
        let day_of_year = self.0.rem_euclid(DAYS_PER_YEAR);
        CalendarDate {
            year,
            month: (day_of_year / DAYS_PER_MONTH + 1) as u8,
            day: (day_of_year % DAYS_PER_MONTH + 1) as u8,
        }
    }

    /// Whether this date is the first day of a month.
    pub fn is_month_start(self) -> bool {
        self.calendar().day == 1
    }

    /// Whether this date is the first day of a year.
    pub fn is_year_start(self) -> bool {
        let calendar = self.calendar();
        calendar.month == 1 && calendar.day == 1
    }
}

impl fmt::Display for GameDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.calendar().fmt(f)
    }
}

/// A date in year/month/day form. Months and days are one-based.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct CalendarDate {
    /// Year since the epoch; scenarios pick their own numbering.
    pub year: i64,
    /// Month of the year, `1..=12`.
    pub month: u8,
    /// Day of the month, `1..=30`.
    pub day: u8,
}

/// An out-of-range month or day in a [`CalendarDate`].
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid calendar date: month {month} day {day}")]
pub struct InvalidCalendarDate {
    /// The rejected month value.
    pub month: u8,
    /// The rejected day value.
    pub day: u8,
}

impl CalendarDate {
    /// Converts to a day count, validating month and day ranges.
    pub fn to_date(self) -> Result<GameDate, InvalidCalendarDate> {
        if !(1..=MONTHS_PER_YEAR as u8).contains(&self.month)
            || !(1..=DAYS_PER_MONTH as u8).contains(&self.day)
        {
            return Err(InvalidCalendarDate {
                month: self.month,
                day: self.day,
            });
        }
        Ok(GameDate(
            self.year * DAYS_PER_YEAR
                + (i64::from(self.month) - 1) * DAYS_PER_MONTH
                + (i64::from(self.day) - 1),
        ))
    }
}

impl fmt::Display for CalendarDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}.{:02}", self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_year_zero_month_one_day_one() {
        let calendar = GameDate::EPOCH.calendar();
        assert_eq!((calendar.year, calendar.month, calendar.day), (0, 1, 1));
        assert!(GameDate::EPOCH.is_month_start());
        assert!(GameDate::EPOCH.is_year_start());
    }

    #[test]
    fn year_boundaries_roll_over() {
        let last_day = GameDate::from_days(DAYS_PER_YEAR - 1).calendar();
        assert_eq!((last_day.year, last_day.month, last_day.day), (0, 12, 30));
        let new_year = GameDate::from_days(DAYS_PER_YEAR).calendar();
        assert_eq!((new_year.year, new_year.month, new_year.day), (1, 1, 1));
    }

    #[test]
    fn dates_before_the_epoch_resolve_correctly() {
        let eve = GameDate::from_days(-1).calendar();
        assert_eq!((eve.year, eve.month, eve.day), (-1, 12, 30));
    }

    #[test]
    fn round_trip_through_calendar_form() {
        for days in [-800, -1, 0, 1, 29, 30, 359, 360, 100_000] {
            let date = GameDate::from_days(days);
            assert_eq!(date.calendar().to_date().unwrap(), date);
        }
    }

    #[test]
    fn invalid_calendar_dates_are_rejected() {
        let invalid = CalendarDate {
            year: 1,
            month: 13,
            day: 1,
        };
        assert!(invalid.to_date().is_err());
        let invalid_day = CalendarDate {
            year: 1,
            month: 1,
            day: 31,
        };
        assert!(invalid_day.to_date().is_err());
    }

    #[test]
    fn display_is_zero_padded() {
        let date = CalendarDate {
            year: 411,
            month: 3,
            day: 4,
        }
        .to_date()
        .unwrap();
        assert_eq!(date.to_string(), "411.03.04");
    }
}
