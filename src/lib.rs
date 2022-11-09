//! Collection of utilities for NPL receivers.

//! Build with no_std for embedded platforms.
#![cfg_attr(not(test), no_std)]

use radio_datetime_utils::RadioDateTimeUtils;

/// Limit for spike detection in microseconds, fine tune
const SPIKE_LIMIT: u32 = 30_000;
/// Maximum time in microseconds for a bit to be considered 0 (0/x cases)
const ACTIVE_0_LIMIT: u32 = 150_000;
/// Maximum time in microseconds for bit A to be considered 1
const ACTIVE_A_LIMIT: u32 = 250_000;
/// Maximum time in microseconds for bit A and B to te considered 1
const ACTIVE_AB_LIMIT: u32 = 350_000;
/// Maximum time in microseconds for a minute marker to be detected
const MINUTE_LIMIT: u32 = 550_000;
/// Time in microseconds that a seconds takes
const SECOND: u32 = 1_000_000;
/// Signal is considered lost after this many microseconds
const PASSIVE_RUNAWAY: u32 = 1_500_000;

/// NPL decoder class
pub struct NPLUtils {
    first_minute: bool,
    new_minute: bool,
    new_second: bool,
    second: u8,
    bit_buffer_a: [Option<bool>; 61],
    bit_buffer_b: [Option<bool>; 61],
    radio_datetime: RadioDateTimeUtils,
    parity_1: Option<bool>,
    parity_2: Option<bool>,
    parity_3: Option<bool>,
    parity_4: Option<bool>,
    // below for handle_new_edge()
    before_first_edge: bool,
    t0: u32,
    old_t_diff: u32,
}

impl NPLUtils {
    pub fn new() -> Self {
        Self {
            first_minute: true,
            new_minute: false,
            new_second: false,
            second: 0,
            bit_buffer_a: [None; 61],
            bit_buffer_b: [None; 61],
            radio_datetime: RadioDateTimeUtils::new(0),
            parity_1: None,
            parity_2: None,
            parity_3: None,
            parity_4: None,
            before_first_edge: true,
            t0: 0,
            old_t_diff: 0,
        }
    }

    /// Return if this is the first minute that is decoded.
    pub fn get_first_minute(&self) -> bool {
        self.first_minute
    }

    /// Return if a new minute has arrived.
    pub fn get_new_minute(&self) -> bool {
        self.new_minute
    }

    /// Force the arrival of a new minute.
    ///
    /// This could be useful when reading from a log file.
    ///
    /// This method must be called _before_ `increase_second()`
    pub fn force_new_minute(&mut self) {
        self.new_minute = true;
    }

    /// Return if a new second has arrived.
    pub fn get_new_second(&self) -> bool {
        self.new_second
    }

    /// Get the second counter.
    pub fn get_second(&self) -> u8 {
        self.second
    }

    /// Get the value of the current A bit.
    pub fn get_current_bit_a(&self) -> Option<bool> {
        self.bit_buffer_a[self.second as usize]
    }

    /// Get the value of the current B bit.
    pub fn get_current_bit_b(&self) -> Option<bool> {
        self.bit_buffer_b[self.second as usize]
    }

    /// Set the value of the current A bit and clear the flag indicating arrival of a new minute.
    ///
    /// This could be useful when reading from a log file.
    ///
    /// This method must be called _before_ `increase_second()`.
    ///
    /// # Arguments
    /// * `value` - the value to set the current bit to
    pub fn set_current_bit_a(&mut self, value: Option<bool>) {
        self.bit_buffer_a[self.second as usize] = value;
        self.new_minute = false;
    }

    /// Set the value of the current B bit and clear the flag indicating arrival of a new minute.
    ///
    /// This could be useful when reading from a log file.
    ///
    /// This method must be called _before_ `increase_second()`.
    ///
    /// # Arguments
    /// * `value` - the value to set the current bit to
    pub fn set_current_bit_b(&mut self, value: Option<bool>) {
        self.bit_buffer_b[self.second as usize] = value;
        self.new_minute = false;
    }

    /// Get a copy of the date/time structure.
    pub fn get_radio_datetime(&self) -> RadioDateTimeUtils {
        self.radio_datetime
    }

    /// Get the minute year bit, Some(true) means OK.
    pub fn get_parity_1(&self) -> Option<bool> {
        self.parity_1
    }

    /// Get the hour month/day bit, Some(true) means OK.
    pub fn get_parity_2(&self) -> Option<bool> {
        self.parity_2
    }

    /// Get the weekday parity bit, Some(true) means OK.
    pub fn get_parity_3(&self) -> Option<bool> {
        self.parity_3
    }

    /// Get the hour/minute parity bit, Some(true) means OK.
    pub fn get_parity_4(&self) -> Option<bool> {
        self.parity_4
    }

    /**
     * Determine the bit value if a new edge is received. indicates reception errors,
     * and checks if a new minute has started.
     *
     * This function can deal with spikes, which are arbitrarily set to `SPIKE_LIMIT` microseconds.
     *
     * This method must be called _before_ `increase_second()`.
     *
     * # Arguments
     * * `is_low_edge` - indicates that the edge has gone from high to low (as opposed to
     *                   low-to-high).
     * * `t` - time stamp of the received edge, in microseconds
     */
    pub fn handle_new_edge(&mut self, is_low_edge: bool, t: u32) {
        if self.before_first_edge {
            self.before_first_edge = false;
            self.t0 = t;
            return;
        }
        let t_diff = radio_datetime_utils::time_diff(self.t0, t);
        if t_diff < SPIKE_LIMIT {
            // Shift t0 to deal with a train of spikes adding up to more than `SPIKE_LIMIT` microseconds.
            self.t0 += t_diff;
            return; // random positive or negative spike, ignore
        }
        self.new_minute = false;
        self.t0 = t;
        if is_low_edge {
            self.new_second = false;
            if t_diff < ACTIVE_0_LIMIT {
                if self.old_t_diff > 0 && self.old_t_diff < ACTIVE_0_LIMIT {
                    self.bit_buffer_a[self.second as usize] = Some(false);
                    self.bit_buffer_b[self.second as usize] = Some(true);
                } else if self.old_t_diff == 0 || self.old_t_diff > SECOND - ACTIVE_0_LIMIT {
                    self.bit_buffer_a[self.second as usize] = Some(false);
                    self.bit_buffer_b[self.second as usize] = Some(false);
                }
            } else if t_diff < ACTIVE_A_LIMIT && self.old_t_diff > SECOND - ACTIVE_A_LIMIT {
                self.bit_buffer_a[self.second as usize] = Some(true);
                self.bit_buffer_b[self.second as usize] = Some(false);
            } else if t_diff < ACTIVE_AB_LIMIT && self.old_t_diff > SECOND - ACTIVE_AB_LIMIT {
                self.bit_buffer_a[self.second as usize] = Some(true);
                self.bit_buffer_b[self.second as usize] = Some(true);
            } else if t_diff < MINUTE_LIMIT && self.old_t_diff > SECOND - MINUTE_LIMIT {
                self.new_minute = true;
            } else {
                self.bit_buffer_a[self.second as usize] = None;
                self.bit_buffer_b[self.second as usize] = None;
            }
        } else if t_diff < PASSIVE_RUNAWAY {
            self.new_second = t_diff > ACTIVE_0_LIMIT;
        } else {
            self.bit_buffer_a[self.second as usize] = None;
            self.bit_buffer_b[self.second as usize] = None;
        }
        self.old_t_diff = t_diff;
    }

    /// Determine the length of this minute in bits.
    // TODO determine position of 0111_1110 end-of-minute marker and consequently add -1, 0, 1
    pub fn get_minute_length(&self) -> u8 {
        60
    }

    /// Increase or reset `second` and clear `first_minute` when appropriate.
    ///
    /// This method must be called _after_ `decode_time()`, `handle_new_edge()`,
    /// `set_current_bit_a()`, `set_current_bit_b()`, and `force_new_minute()`.
    pub fn increase_second(&mut self) {
        let minute_length = self.get_minute_length();
        if self.new_minute {
            if self.first_minute
                && self.second == minute_length
                // check bit train 0111_1110
                // check DST is_some()
                && self.radio_datetime.get_year().is_some()
                && self.radio_datetime.get_month().is_some()
                && self.radio_datetime.get_day().is_some()
                && self.radio_datetime.get_weekday().is_some()
                && self.radio_datetime.get_hour().is_some()
                && self.radio_datetime.get_minute().is_some()
            {
                // allow displaying of information after the first properly decoded minute
                self.first_minute = false;
            }
            self.second = 0;
        } else {
            self.second += 1;
            // wrap in case we missed the minute marker to prevent index-out-of-range
            if self.second == minute_length + 1 {
                self.second = 0;
            }
        }
    }

    /// Decode the time broadcast during the last minute.
    ///
    /// This method must be called _before_ `increase_second()`
    pub fn decode_time(&mut self) {
        let minute_length = self.get_minute_length();
        let mut added_minute = false;
        if !self.first_minute {
            added_minute = self.radio_datetime.add_minute();
        }
        if self.second == minute_length {
            self.parity_1 =
                radio_datetime_utils::get_parity(&self.bit_buffer_a, 17, 24, self.bit_buffer_b[54]);
            self.radio_datetime.set_year(
                radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 24, 17),
                self.parity_1 == Some(true),
                added_minute && !self.first_minute,
            );

            self.parity_2 =
                radio_datetime_utils::get_parity(&self.bit_buffer_a, 25, 35, self.bit_buffer_b[55]);
            self.radio_datetime.set_month(
                radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 29, 25),
                self.parity_2 == Some(true),
                added_minute && !self.first_minute,
            );

            self.parity_3 =
                radio_datetime_utils::get_parity(&self.bit_buffer_a, 36, 38, self.bit_buffer_b[56]);
            self.radio_datetime.set_weekday(
                radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 38, 36),
                self.parity_3 == Some(true),
                added_minute && !self.first_minute,
            );

            self.radio_datetime.set_day(
                radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 35, 30),
                self.parity_1 == Some(true)
                    && self.parity_2 == Some(true)
                    && self.parity_3 == Some(true),
                added_minute && !self.first_minute,
            );

            self.parity_4 =
                radio_datetime_utils::get_parity(&self.bit_buffer_a, 39, 51, self.bit_buffer_b[57]);
            self.radio_datetime.set_hour(
                radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 44, 39),
                self.parity_4 == Some(true),
                added_minute && !self.first_minute,
            );
            self.radio_datetime.set_minute(
                radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 51, 45),
                self.parity_4 == Some(true),
                added_minute && !self.first_minute,
            );

            self.radio_datetime.set_dst(
                self.bit_buffer_b[58],
                self.bit_buffer_b[53],
                added_minute && !self.first_minute,
            );

            self.radio_datetime.bump_minutes_running();
        }
    }
}

impl Default for NPLUtils {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::NPLUtils;
    use radio_datetime_utils::{DST_ANNOUNCED, DST_PROCESSED, DST_SUMMER};

    const BIT_BUFFER_A: [bool; 60] = [
        true, // begin-of-minute marker
        false, false, false, false, false, false, false, false, // unused 1-8
        false, false, false, false, false, false, false, false, // unused 9-16
        false, false, true, false, false, false, true, false, // year 22
        true, false, false, false, false, // month 10
        true, false, false, false, true, true, // day 23
        true, true, false, // Saturday
        false, true, false, true, false, false, // hour 14
        true, false, true, true, false, false, false, // minute 58
        false, true, true, true, true, true, true, false, // bit train
    ];
    const BIT_BUFFER_B: [bool; 60] = [
        true, // begin-of-minute marker,
        false, false, false, false, false, false, false, false, // DUT1 positive
        false, false, false, false, false, false, false, false, // DUT1 negative
        false, false, false, false, false, false, false, false, // unused 17-24
        false, false, false, false, false, false, false, false, // unused 25-32
        false, false, false, false, false, false, false, false, // unused 33-40
        false, false, false, false, false, false, false, false, // unused 41-48
        false, false, false, false, // unused 49-52
        false, // summer time warning
        true,  // year parity
        true,  // month+day parity
        true,  // weekday parity
        false, // hour+minute parity
        true,  // summer time active
        false, // unused
    ];
    #[test]
    fn test_new_edge() {
        // TODO implement
    }

    #[test]
    fn test_decode_time_incomplete_minute() {
        let mut npl = NPLUtils::default();
        assert_eq!(npl.first_minute, true);
        npl.second = 42;
        // note that npl.bit_buffer_[ab] are still empty
        assert_ne!(npl.get_minute_length(), npl.second);
        assert_eq!(npl.parity_1, None);
        npl.decode_time();
        // not enough seconds in this minute, so nothing should happen:
        assert_eq!(npl.parity_1, None);
    }
    #[test]
    fn test_decode_time_complete_minute_ok() {
        let mut npl = NPLUtils::default();
        npl.second = 60;
        assert_eq!(npl.get_minute_length(), npl.second);
        for b in 0..=59 {
            npl.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            npl.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        npl.decode_time();
        // we should have a valid decoding:
        assert_eq!(npl.radio_datetime.get_minute(), Some(58));
        assert_eq!(npl.radio_datetime.get_hour(), Some(14));
        assert_eq!(npl.radio_datetime.get_weekday(), Some(6));
        assert_eq!(npl.radio_datetime.get_day(), Some(23));
        assert_eq!(npl.radio_datetime.get_month(), Some(10));
        assert_eq!(npl.radio_datetime.get_year(), Some(22));
        assert_eq!(npl.parity_1, Some(true));
        assert_eq!(npl.parity_2, Some(true));
        assert_eq!(npl.parity_3, Some(true));
        assert_eq!(npl.parity_4, Some(true));
        assert_eq!(npl.radio_datetime.get_dst(), Some(DST_SUMMER));
        assert_eq!(npl.radio_datetime.get_leap_second(), None); // not available
    }
    #[test]
    fn test_decode_time_complete_minute_bad_bits() {
        let mut npl = NPLUtils::default();
        npl.second = 60;
        assert_eq!(npl.get_minute_length(), npl.second);
        for b in 0..=59 {
            npl.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            npl.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        // introduce some distortions:
        npl.bit_buffer_a[31] = None; // None hour
        npl.bit_buffer_a[48] = Some(!npl.bit_buffer_a[48].unwrap());
        npl.decode_time();
        assert_eq!(npl.radio_datetime.get_minute(), None); // bad parity and first decoding
        assert_eq!(npl.radio_datetime.get_hour(), None); // bad parity and first decoding
        assert_eq!(npl.radio_datetime.get_weekday(), Some(6));
        assert_eq!(npl.radio_datetime.get_day(), None); // broken bit
        assert_eq!(npl.radio_datetime.get_month(), None); // broken parity and first decoding
        assert_eq!(npl.radio_datetime.get_year(), Some(22));
        assert_eq!(npl.parity_1, Some(true));
        assert_eq!(npl.parity_2, None); // broken bit
        assert_eq!(npl.parity_3, Some(true));
        assert_eq!(npl.parity_4, Some(false)); // bad parity
        assert_eq!(npl.radio_datetime.get_dst(), Some(DST_SUMMER));
        assert_eq!(npl.radio_datetime.get_leap_second(), None);
    }
    #[test]
    fn continue_decode_time_complete_minute_jumped_values() {
        let mut npl = NPLUtils::default();
        npl.second = 60;
        assert_eq!(npl.get_minute_length(), npl.second);
        for b in 0..=59 {
            npl.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            npl.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        npl.decode_time();
        assert_eq!(npl.radio_datetime.get_minute(), Some(58));
        assert_eq!(npl.radio_datetime.get_jump_minute(), false);
        npl.first_minute = false;
        // minute 58 is really cool, so do not update bit 51 (and 57)
        npl.decode_time();
        assert_eq!(npl.radio_datetime.get_minute(), Some(58));
        assert_eq!(npl.radio_datetime.get_hour(), Some(14));
        assert_eq!(npl.radio_datetime.get_weekday(), Some(6));
        assert_eq!(npl.radio_datetime.get_day(), Some(23));
        assert_eq!(npl.radio_datetime.get_month(), Some(10));
        assert_eq!(npl.radio_datetime.get_year(), Some(22));
        assert_eq!(npl.parity_1, Some(true));
        assert_eq!(npl.parity_2, Some(true));
        assert_eq!(npl.parity_3, Some(true));
        assert_eq!(npl.parity_4, Some(true));
        assert_eq!(npl.radio_datetime.get_dst(), Some(DST_SUMMER));
        assert_eq!(npl.radio_datetime.get_leap_second(), None);
        assert_eq!(npl.radio_datetime.get_jump_minute(), true);
        assert_eq!(npl.radio_datetime.get_jump_hour(), false);
        assert_eq!(npl.radio_datetime.get_jump_weekday(), false);
        assert_eq!(npl.radio_datetime.get_jump_day(), false);
        assert_eq!(npl.radio_datetime.get_jump_month(), false);
        assert_eq!(npl.radio_datetime.get_jump_year(), false);
    }
    #[test]
    fn continue_decode_time_complete_minute_bad_bits() {
        let mut npl = NPLUtils::default();
        npl.second = 60;
        assert_eq!(npl.get_minute_length(), npl.second);
        for b in 0..=59 {
            npl.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            npl.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        npl.decode_time();
        npl.first_minute = false;
        // update for the next minute:
        npl.bit_buffer_a[51] = Some(true);
        npl.bit_buffer_b[57] = Some(true);
        // introduce some distortions:
        npl.bit_buffer_a[31] = None; // None hour
        npl.bit_buffer_a[48] = Some(!npl.bit_buffer_a[48].unwrap());
        npl.decode_time();
        assert_eq!(npl.radio_datetime.get_minute(), Some(59)); // bad parity
        assert_eq!(npl.radio_datetime.get_hour(), Some(14));
        assert_eq!(npl.radio_datetime.get_weekday(), Some(6)); // broken parity
        assert_eq!(npl.radio_datetime.get_day(), Some(23)); // broken bit
        assert_eq!(npl.radio_datetime.get_month(), Some(10)); // broken parity
        assert_eq!(npl.radio_datetime.get_year(), Some(22)); // broken parity
        assert_eq!(npl.parity_1, Some(true));
        assert_eq!(npl.parity_2, None); // broken bit
        assert_eq!(npl.parity_3, Some(true));
        assert_eq!(npl.parity_4, Some(false)); // bad parity
        assert_eq!(npl.radio_datetime.get_dst(), Some(DST_SUMMER));
        assert_eq!(npl.radio_datetime.get_leap_second(), None);
        assert_eq!(npl.radio_datetime.get_jump_minute(), false);
        assert_eq!(npl.radio_datetime.get_jump_hour(), false);
        assert_eq!(npl.radio_datetime.get_jump_weekday(), false);
        assert_eq!(npl.radio_datetime.get_jump_day(), false);
        assert_eq!(npl.radio_datetime.get_jump_month(), false);
        assert_eq!(npl.radio_datetime.get_jump_year(), false);
    }
    #[test]
    fn continue_decode_time_complete_minute_dst_change() {
        let mut npl = NPLUtils::default();
        npl.second = 60;
        for b in 0..=59 {
            npl.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            npl.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        // DST change must be at top of hour and
        // announcements only count before the hour, so set minute to 59:
        npl.bit_buffer_a[51] = Some(true);
        npl.bit_buffer_b[57] = Some(true);
        // announce a DST change:
        npl.bit_buffer_b[53] = Some(true);
        npl.decode_time();
        assert_eq!(npl.radio_datetime.get_minute(), Some(59));
        assert_eq!(
            npl.radio_datetime.get_dst(),
            Some(DST_ANNOUNCED | DST_SUMMER)
        );
        // next minute and hour:
        npl.bit_buffer_a[45] = Some(false);
        npl.bit_buffer_a[47] = Some(false);
        npl.bit_buffer_a[48] = Some(false);
        npl.bit_buffer_a[51] = Some(false);
        npl.bit_buffer_a[44] = Some(true);
        npl.bit_buffer_b[57] = Some(false);
        // which will have a DST change:
        npl.bit_buffer_b[53] = Some(true);
        npl.bit_buffer_b[58] = Some(false);
        // leave npl.fist_minute true on purpose to catch minute-length bugs
        npl.decode_time();
        assert_eq!(npl.radio_datetime.get_minute(), Some(0));
        assert_eq!(npl.radio_datetime.get_hour(), Some(15));
        assert_eq!(npl.radio_datetime.get_dst(), Some(DST_PROCESSED)); // DST flipped off
    }

    #[test]
    fn test_increase_second_same_minute_ok() {
        let mut npl = NPLUtils::default();
        npl.second = 37;
        // all date/time values are None
        npl.increase_second();
        assert_eq!(npl.first_minute, true);
        assert_eq!(npl.second, 38);
    }
    #[test]
    fn test_increase_second_same_minute_overflow() {
        let mut npl = NPLUtils::default();
        npl.second = 60;
        // leap second value is None, or 0111_1110 is "in the middle"
        npl.increase_second();
        assert_eq!(npl.first_minute, true);
        assert_eq!(npl.second, 0);
    }
    #[test]
    fn test_increase_second_new_minute_ok() {
        let mut npl = NPLUtils::default();
        npl.new_minute = true;
        npl.second = 60;
        npl.radio_datetime.set_year(Some(22), true, false);
        npl.radio_datetime.set_month(Some(10), true, false);
        npl.radio_datetime.set_weekday(Some(6), true, false);
        npl.radio_datetime.set_day(Some(22), true, false);
        npl.radio_datetime.set_hour(Some(12), true, false);
        npl.radio_datetime.set_minute(Some(59), true, false);
        npl.radio_datetime.set_dst(Some(true), Some(false), false);
        // leap second value is None
        npl.increase_second();
        assert_eq!(npl.first_minute, false);
        assert_eq!(npl.second, 0);
    }
    #[test]
    fn test_increase_second_new_minute_none_values() {
        let mut npl = NPLUtils::default();
        npl.new_minute = true;
        npl.second = 60;
        // all date/time values left None
        npl.increase_second();
        assert_eq!(npl.first_minute, true);
        assert_eq!(npl.second, 0);
    }
}
