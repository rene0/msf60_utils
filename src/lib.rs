//! Collection of utilities for MSF receivers.

//! Build with no_std for embedded platforms.
#![cfg_attr(not(test), no_std)]

use core::cmp::Ordering;
use radio_datetime_utils::{radio_datetime_helpers, RadioDateTimeUtils};

pub mod msf_helpers;

/// Default upper limit for spike detection in microseconds
const SPIKE_LIMIT: u32 = 30_000;
/// Maximum time in microseconds for a bit to be considered 0 (0/x cases)
const ACTIVE_0_LIMIT: u32 = 150_000;
/// Maximum time in microseconds for bit A to be considered 1
const ACTIVE_A_LIMIT: u32 = 250_000;
/// Maximum time in microseconds for bit A and B to te considered 1
const ACTIVE_AB_LIMIT: u32 = 350_000;
/// Maximum time in microseconds for a minute marker to be detected
const MINUTE_LIMIT: u32 = 550_000;
/// Signal is considered lost after this many microseconds
const PASSIVE_RUNAWAY: u32 = 1_500_000;

/// MSF decoder class
pub struct MSFUtils {
    first_minute: bool,
    new_minute: bool,      // 0111_1110 marker seen
    past_new_minute: bool, // long bit at begin-of-minute seen
    new_second: bool,
    second: u8,
    bit_buffer_a: [Option<bool>; radio_datetime_utils::BIT_BUFFER_SIZE],
    bit_buffer_b: [Option<bool>; radio_datetime_utils::BIT_BUFFER_SIZE],
    radio_datetime: RadioDateTimeUtils,
    parity_1: Option<bool>,
    parity_2: Option<bool>,
    parity_3: Option<bool>,
    parity_4: Option<bool>,
    dut1: Option<i8>, // DUT1 in deci-seconds
    // below for handle_new_edge()
    before_first_edge: bool,
    t0: u32,
    old_t_diff: u32,
    spike_limit: u32,
}

impl MSFUtils {
    pub fn new() -> Self {
        Self {
            first_minute: true,
            new_minute: false,
            past_new_minute: false,
            new_second: false,
            second: 0,
            bit_buffer_a: [None; radio_datetime_utils::BIT_BUFFER_SIZE],
            bit_buffer_b: [None; radio_datetime_utils::BIT_BUFFER_SIZE],
            radio_datetime: RadioDateTimeUtils::new(0),
            parity_1: None,
            parity_2: None,
            parity_3: None,
            parity_4: None,
            dut1: None,
            before_first_edge: true,
            t0: 0,
            old_t_diff: 0,
            spike_limit: SPIKE_LIMIT,
        }
    }

    /// Return if this is the first minute that is decoded.
    pub fn get_first_minute(&self) -> bool {
        self.first_minute
    }

    /// Return if a new minute (0111_1110 marker) has arrived.
    pub fn get_new_minute(&self) -> bool {
        self.new_minute
    }

    /// Return if the 500 ms long begin-of-minute marker has arrived.
    pub fn get_past_new_minute(&self) -> bool {
        self.past_new_minute
    }

    /// Force the arrival of a new minute (0111_1110 version).
    ///
    /// This could be useful when reading from a log file.
    ///
    /// This method must be called _before_ `increase_second()`
    pub fn force_new_minute(&mut self) {
        self.new_minute = true;
        self.past_new_minute = false;
    }

    /// Force the arrival of a new minute (begin-of-minute version).
    ///
    /// This could be useful when reading from a log file.
    ///
    /// This method must be called _before_ `increase_second()`
    pub fn force_past_new_minute(&mut self) {
        self.new_minute = false;
        self.past_new_minute = true;
        self.second = 0;
        self.bit_buffer_a[0] = Some(true);
        self.bit_buffer_b[0] = Some(true);
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
        self.past_new_minute = false;
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
        self.past_new_minute = false;
    }

    /// Get a copy of the date/time structure.
    pub fn get_radio_datetime(&self) -> RadioDateTimeUtils {
        self.radio_datetime
    }

    /// Get the year parity bit, Some(true) means OK.
    pub fn get_parity_1(&self) -> Option<bool> {
        self.parity_1
    }

    /// Get the month/day parity bit, Some(true) means OK.
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

    /// Get the value of DUT1 (UT1 - UTC) in deci-seconds.
    pub fn get_dut1(&self) -> Option<i8> {
        self.dut1
    }

    /// Return the current spike limit in microseconds.
    pub fn get_spike_limit(&self) -> u32 {
        self.spike_limit
    }

    /// Set the new spike limit in microseconds, [0(off)..ACTIVE_0_LIMIT)
    ///
    /// # Arguments
    /// * `value` - the value to set the spike limit to.
    pub fn set_spike_limit(&mut self, value: u32) {
        if value < ACTIVE_0_LIMIT {
            self.spike_limit = value;
        }
    }

    /// Determine the bit value if a new edge is received. indicates reception errors,
    /// and checks if a new minute has started.
    ///
    /// This function can deal with spikes, which are arbitrarily set to `spike_limit` microseconds.
    ///
    /// This method must be called _before_ `increase_second()`.
    ///
    /// # Arguments
    /// * `is_low_edge` - indicates that the edge has gone from high to low (as opposed to
    ///                   low-to-high).
    /// * `t` - time stamp of the received edge, in microseconds
    pub fn handle_new_edge(&mut self, is_low_edge: bool, t: u32) {
        if self.before_first_edge {
            self.before_first_edge = false;
            self.t0 = t;
            return;
        }
        let t_diff = radio_datetime_helpers::time_diff(self.t0, t);
        if t_diff < self.spike_limit {
            // Shift t0 to deal with a train of spikes adding up to more than `spike_limit` microseconds.
            self.t0 += t_diff;
            return; // random positive or negative spike, ignore
        }
        self.new_minute = false;
        self.past_new_minute = false;
        self.t0 = t;
        if is_low_edge {
            self.new_second = false;
            if t_diff < ACTIVE_0_LIMIT {
                if self.old_t_diff > 0 && self.old_t_diff < ACTIVE_0_LIMIT {
                    self.bit_buffer_a[self.second as usize] = Some(false);
                    self.bit_buffer_b[self.second as usize] = Some(true);
                } else if self.old_t_diff > 1_000_000 - MINUTE_LIMIT {
                    self.bit_buffer_a[self.second as usize] = Some(false);
                    self.bit_buffer_b[self.second as usize] = Some(false);
                }
                self.new_minute = self.end_of_minute_marker_present();
            } else if t_diff < ACTIVE_A_LIMIT && self.old_t_diff > 1_000_000 - ACTIVE_AB_LIMIT {
                self.bit_buffer_a[self.second as usize] = Some(true);
                self.bit_buffer_b[self.second as usize] = Some(false);
            } else if t_diff < ACTIVE_AB_LIMIT && self.old_t_diff > 1_000_000 - ACTIVE_AB_LIMIT {
                self.bit_buffer_a[self.second as usize] = Some(true);
                self.bit_buffer_b[self.second as usize] = Some(true);
            } else if t_diff < MINUTE_LIMIT && self.old_t_diff > 1_000_000 - ACTIVE_AB_LIMIT {
                self.past_new_minute = true;
                self.second = 0;
                self.bit_buffer_a[0] = Some(true);
                self.bit_buffer_b[0] = Some(true);
            } else {
                // active runaway or first low edge
                self.bit_buffer_a[self.second as usize] = None;
                self.bit_buffer_b[self.second as usize] = None;
            }
        } else if t_diff < PASSIVE_RUNAWAY {
            self.new_second = t_diff > 1_000_000 - MINUTE_LIMIT;
        } else {
            self.bit_buffer_a[self.second as usize] = None;
            self.bit_buffer_b[self.second as usize] = None;
        }
        self.old_t_diff = t_diff;
    }

    /// Determine the length of this minute in seconds.
    pub fn get_minute_length(&self) -> u8 {
        if (58..=60).contains(&self.second) && self.search_eom_marker(false) {
            self.second + 1
        } else if self.second == 59 && self.search_eom_marker(true) {
            61
        } else {
            60
        }
    }

    /// Return if the end-of-minute marker (0111_1110) is present at the end of the A bits.
    ///
    /// This method must be called _before_ `increase_second()`
    pub fn end_of_minute_marker_present(&self) -> bool {
        self.search_eom_marker(false)
    }

    /// Helper for end_of_minute_marker_present() and get_minute_length()
    fn search_eom_marker(&self, predict: bool) -> bool {
        if self.second < 7 {
            return false; // not enough bits to test
        }
        const MARKER: [bool; 8] = [false, true, true, true, true, true, true, false];
        for (idx, bit) in self.bit_buffer_a
            [(self.second - 7 + predict as u8) as usize..=self.second as usize]
            .iter()
            .enumerate()
        {
            if bit.is_none() || *bit != Some(MARKER[idx]) {
                return false;
            }
        }
        true
    }

    /// Increase or reset `second`.
    ///
    /// Returns if the second counter was increased/wrapped normally (true)
    /// or due to an overflow (false).
    ///
    /// This method must be called _after_ `decode_time()`, `handle_new_edge()`,
    /// `set_current_bit_a()`, `set_current_bit_b()`, `end_of_minute_marker_present()`
    /// and `force_new_minute()`.
    pub fn increase_second(&mut self) -> bool {
        let minute_length = self.get_minute_length();
        RadioDateTimeUtils::increase_second(&mut self.second, self.new_minute, minute_length)
    }

    /// Call add_minute() on `self.radio_datetime` and passes on that result.
    ///
    /// This could be useful for consumers just wanting to advance their current date/time.
    pub fn add_minute(&mut self) -> bool {
        self.radio_datetime.clear_jumps();
        self.radio_datetime.add_minute()
    }

    /// Decode the time broadcast during the last minute and clear `first_minute` when appropriate.
    ///
    /// This method must be called _before_ `increase_second()`
    ///
    /// # Arguments
    /// * `strict_checks` - checks all parities, DUT1 validity, and EOM marker presence when setting
    ///                     date/time and clearing self.first_minute
    pub fn decode_time(&mut self, strict_checks: bool) {
        self.radio_datetime.clear_jumps();
        let minute_length = self.get_minute_length(); // calculation depends on self.second
        let mut added_minute = false;
        if !self.first_minute {
            added_minute = self.radio_datetime.add_minute();
        }
        if self.second + 1 == minute_length {
            let offset: isize = match 60.cmp(&minute_length) {
                Ordering::Less => 1,
                Ordering::Equal => 0,
                Ordering::Greater => -1,
            };

            self.parity_1 = radio_datetime_helpers::get_parity(
                &self.bit_buffer_a,
                (17 + offset) as usize,
                (24 + offset) as usize,
                self.bit_buffer_b[(54 + offset) as usize],
            );
            self.parity_2 = radio_datetime_helpers::get_parity(
                &self.bit_buffer_a,
                (25 + offset) as usize,
                (35 + offset) as usize,
                self.bit_buffer_b[(55 + offset) as usize],
            );
            self.parity_3 = radio_datetime_helpers::get_parity(
                &self.bit_buffer_a,
                (36 + offset) as usize,
                (38 + offset) as usize,
                self.bit_buffer_b[(56 + offset) as usize],
            );
            self.parity_4 = radio_datetime_helpers::get_parity(
                &self.bit_buffer_a,
                (39 + offset) as usize,
                (51 + offset) as usize,
                self.bit_buffer_b[(57 + offset) as usize],
            );

            self.dut1 = None;
            // bit 16 is dropped in case of a negative leap second
            let stop = if offset == -1 { 15 } else { 16 };
            if let Some(dut1p) = msf_helpers::get_unary_value(&self.bit_buffer_b, 1, 8) {
                if let Some(dut1n) = msf_helpers::get_unary_value(&self.bit_buffer_b, 9, stop) {
                    self.dut1 = if dut1p * dut1n == 0 {
                        Some(dut1p - dut1n)
                    } else {
                        None
                    };
                }
            }

            let strict_ok = self.parity_1 == Some(true)
                && self.parity_2 == Some(true)
                && self.parity_3 == Some(true)
                && self.parity_4 == Some(true)
                && self.dut1.is_some()
                && self.end_of_minute_marker_present();

            self.radio_datetime.set_year(
                radio_datetime_helpers::get_bcd_value(
                    &self.bit_buffer_a,
                    (24 + offset) as usize,
                    (17 + offset) as usize,
                ),
                if strict_checks {
                    strict_ok
                } else {
                    self.parity_1 == Some(true)
                },
                added_minute && !self.first_minute,
            );
            self.radio_datetime.set_month(
                radio_datetime_helpers::get_bcd_value(
                    &self.bit_buffer_a,
                    (29 + offset) as usize,
                    (25 + offset) as usize,
                ),
                if strict_checks {
                    strict_ok
                } else {
                    self.parity_2 == Some(true)
                },
                added_minute && !self.first_minute,
            );
            self.radio_datetime.set_weekday(
                radio_datetime_helpers::get_bcd_value(
                    &self.bit_buffer_a,
                    (38 + offset) as usize,
                    (36 + offset) as usize,
                ),
                if strict_checks {
                    strict_ok
                } else {
                    self.parity_3 == Some(true)
                },
                added_minute && !self.first_minute,
            );
            self.radio_datetime.set_day(
                radio_datetime_helpers::get_bcd_value(
                    &self.bit_buffer_a,
                    (35 + offset) as usize,
                    (30 + offset) as usize,
                ),
                if strict_checks {
                    strict_ok
                } else {
                    self.parity_1 == Some(true)
                        && self.parity_2 == Some(true)
                        && self.parity_3 == Some(true)
                },
                added_minute && !self.first_minute,
            );

            self.radio_datetime.set_hour(
                radio_datetime_helpers::get_bcd_value(
                    &self.bit_buffer_a,
                    (44 + offset) as usize,
                    (39 + offset) as usize,
                ),
                if strict_checks {
                    strict_ok
                } else {
                    self.parity_4 == Some(true)
                },
                added_minute && !self.first_minute,
            );
            self.radio_datetime.set_minute(
                radio_datetime_helpers::get_bcd_value(
                    &self.bit_buffer_a,
                    (51 + offset) as usize,
                    (45 + offset) as usize,
                ),
                if strict_checks {
                    strict_ok
                } else {
                    self.parity_4 == Some(true)
                },
                added_minute && !self.first_minute,
            );

            self.radio_datetime.set_dst(
                self.bit_buffer_b[(58 + offset) as usize],
                self.bit_buffer_b[(53 + offset) as usize],
                added_minute && !self.first_minute,
            );

            if if strict_checks {
                strict_ok
            } else {
                self.dut1.is_some()
            } && self.radio_datetime.is_valid()
            {
                // allow displaying of information after the first properly decoded minute
                self.first_minute = false;
            }

            self.radio_datetime.bump_minutes_running();
        }
    }
}

impl Default for MSFUtils {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        false, true, true, true, true, true, true, false, // end-of-minute marker
    ];
    const BIT_BUFFER_B: [bool; 60] = [
        true, // begin-of-minute marker,
        false, false, false, false, false, false, false, false, // DUT1 positive
        true, true, false, false, false, false, false, false, // DUT1 negative (-2)
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
    fn test_new_edge_bit_0_0() {
        const EDGE_BUFFER: [(bool, u32); 4] = [
            // Some(false,false) bit value
            (!false, 422_994_439), // 0
            (!true, 423_907_610),  // 913_171
            (!false, 423_997_265), // 89_655
            (!true, 424_906_368),  // 909_103
        ];
        let mut msf = MSFUtils::default();
        assert_eq!(msf.before_first_edge, true);
        msf.handle_new_edge(EDGE_BUFFER[0].0, EDGE_BUFFER[0].1);
        assert_eq!(msf.before_first_edge, false);
        assert_eq!(msf.t0, EDGE_BUFFER[0].1); // very first edge

        msf.handle_new_edge(EDGE_BUFFER[1].0, EDGE_BUFFER[1].1); // first significant edge
        assert_eq!(msf.t0, EDGE_BUFFER[1].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), None); // not yet determined, passive part
        assert_eq!(msf.get_current_bit_b(), None); // not yet determined, passive part

        msf.handle_new_edge(EDGE_BUFFER[2].0, EDGE_BUFFER[2].1);
        assert_eq!(msf.t0, EDGE_BUFFER[2].1); // longer than a spike
        assert_eq!(msf.new_second, false);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(false));
        assert_eq!(msf.get_current_bit_b(), Some(false));

        // passive part of second must keep the bit value
        msf.handle_new_edge(EDGE_BUFFER[3].0, EDGE_BUFFER[3].1);
        assert_eq!(msf.t0, EDGE_BUFFER[3].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(false)); // keep bit value
        assert_eq!(msf.get_current_bit_b(), Some(false)); // keep bit value
    }
    #[test]
    fn test_new_edge_bit_0_1() {
        // TODO replace with real data once (0,1) bit pairs are broadcast again
        const EDGE_BUFFER: [(bool, u32); 6] = [
            // Some(false,false) bit value
            (!false, 0),         // 0
            (!true, 920_000),    // 920_000
            (!false, 1_030_000), // 110_000
            (!true, 1_128_000),  // 98_000
            (!false, 1_232_000), // 104_000
            (!true, 1_896_000),  // 644_000
        ];
        let mut msf = MSFUtils::default();
        assert_eq!(msf.before_first_edge, true);
        msf.handle_new_edge(EDGE_BUFFER[0].0, EDGE_BUFFER[0].1);
        assert_eq!(msf.before_first_edge, false);
        assert_eq!(msf.t0, EDGE_BUFFER[0].1); // very first edge

        msf.handle_new_edge(EDGE_BUFFER[1].0, EDGE_BUFFER[1].1); // first significant edge
        assert_eq!(msf.t0, EDGE_BUFFER[1].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), None); // not yet determined, passive part
        assert_eq!(msf.get_current_bit_b(), None); // not yet determined, passive part

        // active signal part one, so we should get an intermediate (0,0) bit pair here:
        msf.handle_new_edge(EDGE_BUFFER[2].0, EDGE_BUFFER[2].1);
        assert_eq!(msf.t0, EDGE_BUFFER[2].1); // longer than a spike
        assert_eq!(msf.new_second, false);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(false));
        assert_eq!(msf.get_current_bit_b(), Some(false));

        // passive signal part one (up to `ACTIVE_0_LIMIT` or 150_000 microseconds), keep bit values:
        msf.handle_new_edge(EDGE_BUFFER[3].0, EDGE_BUFFER[3].1);
        assert_eq!(msf.t0, EDGE_BUFFER[3].1); // longer than a spike
        assert_eq!(msf.new_second, false);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(false));
        assert_eq!(msf.get_current_bit_b(), Some(false));

        // active signal part two, get the (0,1) bit pair:
        msf.handle_new_edge(EDGE_BUFFER[4].0, EDGE_BUFFER[4].1);
        assert_eq!(msf.t0, EDGE_BUFFER[4].1); // longer than a spike
        assert_eq!(msf.new_second, false);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(false));
        assert_eq!(msf.get_current_bit_b(), Some(true));

        // passive signal part two, keep the bit values:
        msf.handle_new_edge(EDGE_BUFFER[5].0, EDGE_BUFFER[5].1);
        assert_eq!(msf.t0, EDGE_BUFFER[5].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(false)); // keep bit value
        assert_eq!(msf.get_current_bit_b(), Some(true)); // keep bit value
    }
    #[test]
    fn test_new_edge_bit_1_0() {
        const EDGE_BUFFER: [(bool, u32); 4] = [
            // Some(true,false) bit value
            (!false, 413_999_083), // 0
            (!true, 414_909_075),  // 918_992
            (!false, 415_090_038), // 180_963
            (!true, 415_908_781),  // 818_743
        ];
        let mut msf = MSFUtils::default();
        assert_eq!(msf.before_first_edge, true);
        msf.handle_new_edge(EDGE_BUFFER[0].0, EDGE_BUFFER[0].1);
        assert_eq!(msf.before_first_edge, false);
        assert_eq!(msf.t0, EDGE_BUFFER[0].1); // very first edge

        msf.handle_new_edge(EDGE_BUFFER[1].0, EDGE_BUFFER[1].1); // first significant edge
        assert_eq!(msf.t0, EDGE_BUFFER[1].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), None); // not yet determined, passive part
        assert_eq!(msf.get_current_bit_b(), None); // not yet determined, passive part

        msf.handle_new_edge(EDGE_BUFFER[2].0, EDGE_BUFFER[2].1);
        assert_eq!(msf.t0, EDGE_BUFFER[2].1); // longer than a spike
        assert_eq!(msf.new_second, false);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(true));
        assert_eq!(msf.get_current_bit_b(), Some(false));

        // passive part of second must keep the bit value
        msf.handle_new_edge(EDGE_BUFFER[3].0, EDGE_BUFFER[3].1);
        assert_eq!(msf.t0, EDGE_BUFFER[3].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(true)); // keep bit value
        assert_eq!(msf.get_current_bit_b(), Some(false)); // keep bit value
    }
    #[test]
    fn test_new_edge_bit_1_1() {
        const EDGE_BUFFER: [(bool, u32); 4] = [
            // Some(true,true) bit value
            (!false, 415_090_038), // 0
            (!true, 415_908_781),  // 818_743
            (!false, 416_194_383), // 285_602
            (!true, 416_901_482),  // 707_099
        ];
        let mut msf = MSFUtils::default();
        assert_eq!(msf.before_first_edge, true);
        msf.handle_new_edge(EDGE_BUFFER[0].0, EDGE_BUFFER[0].1);
        assert_eq!(msf.before_first_edge, false);
        assert_eq!(msf.t0, EDGE_BUFFER[0].1); // very first edge

        msf.handle_new_edge(EDGE_BUFFER[1].0, EDGE_BUFFER[1].1); // first significant edge
        assert_eq!(msf.t0, EDGE_BUFFER[1].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), None); // not yet determined, passive part
        assert_eq!(msf.get_current_bit_b(), None); // not yet determined, passive part

        msf.handle_new_edge(EDGE_BUFFER[2].0, EDGE_BUFFER[2].1);
        assert_eq!(msf.t0, EDGE_BUFFER[2].1); // longer than a spike
        assert_eq!(msf.new_second, false);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(true));
        assert_eq!(msf.get_current_bit_b(), Some(true));

        // passive part of second must keep the bit value
        msf.handle_new_edge(EDGE_BUFFER[3].0, EDGE_BUFFER[3].1);
        assert_eq!(msf.t0, EDGE_BUFFER[3].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(true)); // keep bit value
        assert_eq!(msf.get_current_bit_b(), Some(true)); // keep bit value
    }
    #[test]
    fn test_new_edge_minute() {
        const EDGE_BUFFER: [(bool, u32); 3] = [
            // new minute, (true,true) bit value
            (!false, 420_994_620), // 0
            (!true, 421_906_680),  // 912_060
            (!false, 422_389_442), // 482_762
        ];
        let mut msf = MSFUtils::default();
        assert_eq!(msf.before_first_edge, true);
        msf.handle_new_edge(EDGE_BUFFER[0].0, EDGE_BUFFER[0].1);
        assert_eq!(msf.before_first_edge, false);
        assert_eq!(msf.t0, EDGE_BUFFER[0].1); // very first edge

        msf.handle_new_edge(EDGE_BUFFER[1].0, EDGE_BUFFER[1].1); // first significant edge
        assert_eq!(msf.t0, EDGE_BUFFER[1].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), None); // not yet determined
        assert_eq!(msf.get_current_bit_b(), None); // not yet determined

        msf.handle_new_edge(EDGE_BUFFER[2].0, EDGE_BUFFER[2].1); // new minute
        assert_eq!(msf.t0, EDGE_BUFFER[2].1); // longer than a spike
        assert_eq!(msf.new_second, false);
        assert_eq!(msf.past_new_minute, true);
        assert_eq!(msf.get_current_bit_a(), Some(true));
        assert_eq!(msf.get_current_bit_b(), Some(true));
    }
    #[test]
    fn test_new_edge_active_runaway() {
        const EDGE_BUFFER: [(bool, u32); 3] = [
            // active runaway (broken bit)
            (!false, 417_195_653), // 0
            (!true, 417_908_323),  // 712_670
            (!false, 419_193_216), // 1_284_893
        ];
        let mut msf = MSFUtils::default();
        assert_eq!(msf.before_first_edge, true);
        msf.handle_new_edge(EDGE_BUFFER[0].0, EDGE_BUFFER[0].1);
        assert_eq!(msf.before_first_edge, false);
        assert_eq!(msf.t0, EDGE_BUFFER[0].1); // very first edge

        msf.handle_new_edge(EDGE_BUFFER[1].0, EDGE_BUFFER[1].1); // first significant edge
        assert_eq!(msf.t0, EDGE_BUFFER[1].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), None); // not yet determined, passive part
        assert_eq!(msf.get_current_bit_b(), None); // not yet determined, passive part

        msf.handle_new_edge(EDGE_BUFFER[2].0, EDGE_BUFFER[2].1);
        assert_eq!(msf.t0, EDGE_BUFFER[2].1); // longer than a spike
        assert_eq!(msf.new_second, false);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), None);
        assert_eq!(msf.get_current_bit_b(), None);
    }
    #[test]
    fn test_new_edge_passive_runaway() {
        const EDGE_BUFFER: [(bool, u32); 4] = [
            // passive runaway (transmitter outage?)
            (!false, 897_105_780), // 0
            (!true, 898_042_361),  // 936_581
            (!false, 898_110_362), // 68_001 (0,0) bit
            (!true, 900_067_737),  // 1_957_375 passive runaway
        ];
        let mut msf = MSFUtils::default();
        assert_eq!(msf.before_first_edge, true);
        msf.handle_new_edge(EDGE_BUFFER[0].0, EDGE_BUFFER[0].1);
        assert_eq!(msf.before_first_edge, false);
        assert_eq!(msf.t0, EDGE_BUFFER[0].1); // very first edge

        msf.handle_new_edge(EDGE_BUFFER[1].0, EDGE_BUFFER[1].1); // first significant edge
        assert_eq!(msf.t0, EDGE_BUFFER[1].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), None); // not yet determined, passive part
        assert_eq!(msf.get_current_bit_b(), None); // not yet determined, passive part

        msf.handle_new_edge(EDGE_BUFFER[2].0, EDGE_BUFFER[2].1);
        assert_eq!(msf.t0, EDGE_BUFFER[2].1); // longer than a spike
        assert_eq!(msf.new_second, false);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(false));
        assert_eq!(msf.get_current_bit_b(), Some(false));

        // passive part of second must keep the bit value
        msf.handle_new_edge(EDGE_BUFFER[3].0, EDGE_BUFFER[3].1);
        assert_eq!(msf.t0, EDGE_BUFFER[3].1); // longer than a spike
        assert_eq!(msf.new_second, false);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), None);
        assert_eq!(msf.get_current_bit_b(), None);
    }
    #[test]
    fn test_new_edge_spikes() {
        const EDGE_BUFFER: [(bool, u32); 8] = [
            // spikes
            (!false, 900_122_127), // 0
            (!true, 901_052_140),  // 930_013
            (!false, 901_226_910), // 174_770
            (!true, 902_069_956),  // 843_046
            (!false, 902_085_860), // 15_904
            (!true, 902_105_980),  // 20_120
            (!false, 902_115_859), // 9_879
            (!true, 903_057_346),  // 941_487
        ];
        let mut msf = MSFUtils::default();
        assert_eq!(msf.before_first_edge, true);
        msf.handle_new_edge(EDGE_BUFFER[0].0, EDGE_BUFFER[0].1);
        assert_eq!(msf.before_first_edge, false);
        assert_eq!(msf.t0, EDGE_BUFFER[0].1); // very first edge

        msf.handle_new_edge(EDGE_BUFFER[1].0, EDGE_BUFFER[1].1); // first significant edge
        assert_eq!(msf.t0, EDGE_BUFFER[1].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), None); // not yet determined
        assert_eq!(msf.get_current_bit_b(), None); // not yet determined

        msf.handle_new_edge(EDGE_BUFFER[2].0, EDGE_BUFFER[2].1);
        assert_eq!(msf.t0, EDGE_BUFFER[2].1); // longer than a spike
        assert_eq!(msf.new_second, false);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(true));
        assert_eq!(msf.get_current_bit_b(), Some(false));

        msf.handle_new_edge(EDGE_BUFFER[3].0, EDGE_BUFFER[3].1); // first significant edge
        assert_eq!(msf.t0, EDGE_BUFFER[3].1); // longer than a spike
        assert_eq!(msf.new_second, true);
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(true)); // keep value
        assert_eq!(msf.get_current_bit_b(), Some(false)); // keep value

        // Feed a bunch of spikes of less than spike_limit us, nothing should happen
        let mut spike = msf.t0;
        for i in 4..=6 {
            spike += radio_datetime_helpers::time_diff(EDGE_BUFFER[i - 1].1, EDGE_BUFFER[i].1);
            msf.handle_new_edge(EDGE_BUFFER[i].0, EDGE_BUFFER[i].1);
            assert_eq!(msf.t0, spike);
            assert_eq!(msf.new_second, true);
            assert_eq!(msf.past_new_minute, false);
            assert_eq!(msf.get_current_bit_a(), Some(true));
            assert_eq!(msf.get_current_bit_b(), Some(false));
        }
        msf.handle_new_edge(EDGE_BUFFER[7].0, EDGE_BUFFER[7].1);
        assert_eq!(msf.t0, EDGE_BUFFER[7].1); // longer than a spike
        assert_eq!(msf.new_second, true); // regular new second
        assert_eq!(msf.past_new_minute, false);
        assert_eq!(msf.get_current_bit_a(), Some(true)); // keep value
        assert_eq!(msf.get_current_bit_b(), Some(false)); // keep value
    }

    #[test]
    fn test_eom_marker_too_short() {
        let mut msf = MSFUtils::default();
        msf.second = 5; // something less than 7
        for b in 0..=4 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
        }
        assert_eq!(msf.end_of_minute_marker_present(), false);
    }
    #[test]
    fn test_eom_marker_absent() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        for b in 52..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
        }
        msf.bit_buffer_a[57] = None; // introduce an error
        assert_eq!(msf.end_of_minute_marker_present(), false);
    }
    #[test]
    fn test_eom_marker_present() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        for b in 52..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
        }
        assert_eq!(msf.end_of_minute_marker_present(), true);
    }

    #[test]
    fn test_running_negative_leap_second() {
        let mut msf = MSFUtils::default();
        msf.second = 51;
        for b in 51..=57 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b + 1]);
            assert_eq!(msf.end_of_minute_marker_present(), false); // not done yet
            assert_eq!(msf.get_minute_length(), 60); // default for no EOM marker
            assert_eq!(msf.increase_second(), true);
        }
        assert_eq!(msf.second, 58);
        msf.bit_buffer_a[58] = Some(BIT_BUFFER_A[59]);
        assert_eq!(msf.end_of_minute_marker_present(), true);
        assert_eq!(msf.get_minute_length(), 59); // negative leap second
    }
    #[test]
    fn test_running_no_leap_second() {
        let mut msf = MSFUtils::default();
        msf.second = 52;
        for b in 52..=58 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            assert_eq!(msf.end_of_minute_marker_present(), false); // not done yet
            assert_eq!(msf.get_minute_length(), 60); // default for no EOM marker
            assert_eq!(msf.increase_second(), true);
        }
        assert_eq!(msf.second, 59);
        msf.bit_buffer_a[59] = Some(BIT_BUFFER_A[59]);
        assert_eq!(msf.end_of_minute_marker_present(), true);
        assert_eq!(msf.get_minute_length(), 60); // no leap second
    }
    #[test]
    fn test_running_positive_leap_second() {
        let mut msf = MSFUtils::default();
        msf.second = 53;
        for b in 53..=58 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b - 1]);
            assert_eq!(msf.end_of_minute_marker_present(), false); // not done yet
            assert_eq!(msf.get_minute_length(), 60); // default for no EOM marker
            assert_eq!(msf.increase_second(), true);
        }
        assert_eq!(msf.second, 59);
        msf.bit_buffer_a[59] = Some(BIT_BUFFER_A[58]);
        assert_eq!(msf.end_of_minute_marker_present(), false);
        assert_eq!(msf.search_eom_marker(true), true);
        assert_eq!(msf.get_minute_length(), 61); // positive leap second (without trailing 0 bit)
        assert_eq!(msf.increase_second(), true);
        assert_eq!(msf.second, 60);
        msf.bit_buffer_a[60] = Some(BIT_BUFFER_A[59]);
        assert_eq!(msf.end_of_minute_marker_present(), true);
        assert_eq!(msf.search_eom_marker(true), false);
        assert_eq!(msf.get_minute_length(), 61); // positive leap second (without trailing 0 bit)
    }

    // relaxed checks
    #[test]
    fn test_decode_time_incomplete_minute() {
        let mut msf = MSFUtils::default();
        assert_eq!(msf.first_minute, true);
        msf.second = 42;
        // note that msf.bit_buffer_[ab] are still empty
        assert_ne!(msf.get_minute_length(), msf.second);
        assert_eq!(msf.parity_1, None);
        msf.decode_time(false);
        // not enough seconds in this minute, so nothing should happen:
        assert_eq!(msf.parity_1, None);
    }
    #[test]
    fn test_decode_time_complete_minute_ok() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        assert_eq!(msf.get_minute_length(), msf.second + 1); // EOM marker absent
        for b in 0..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        assert_eq!(msf.end_of_minute_marker_present(), true);
        msf.decode_time(false);
        // we should have a valid decoding:
        assert_eq!(msf.radio_datetime.get_minute(), Some(58));
        assert_eq!(msf.radio_datetime.get_hour(), Some(14));
        assert_eq!(msf.radio_datetime.get_weekday(), Some(6));
        assert_eq!(msf.radio_datetime.get_day(), Some(23));
        assert_eq!(msf.radio_datetime.get_month(), Some(10));
        assert_eq!(msf.radio_datetime.get_year(), Some(22));
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, Some(true));
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(true));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        );
        assert_eq!(msf.radio_datetime.get_leap_second(), None); // not available
        assert_eq!(msf.dut1, Some(-2));
    }
    #[test]
    fn test_decode_time_complete_minute_ok_negative_leap_second() {
        let mut msf = MSFUtils::default();
        msf.second = 58;
        for b in 0..=15 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        // bit 16 removed
        for b in 17..=59 {
            msf.bit_buffer_a[b - 1] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b - 1] = Some(BIT_BUFFER_B[b]);
        }
        assert_eq!(msf.end_of_minute_marker_present(), true);
        assert_eq!(msf.get_minute_length(), msf.second + 1);
        msf.decode_time(false);
        // we should have a valid decoding:
        assert_eq!(msf.radio_datetime.get_minute(), Some(58));
        assert_eq!(msf.radio_datetime.get_hour(), Some(14));
        assert_eq!(msf.radio_datetime.get_weekday(), Some(6));
        assert_eq!(msf.radio_datetime.get_day(), Some(23));
        assert_eq!(msf.radio_datetime.get_month(), Some(10));
        assert_eq!(msf.radio_datetime.get_year(), Some(22));
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, Some(true));
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(true));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        );
        assert_eq!(msf.radio_datetime.get_leap_second(), None); // not available
        assert_eq!(msf.dut1, Some(-2));
        assert_eq!(msf.first_minute, false);
    }
    #[test]
    fn test_decode_time_complete_minute_ok_positive_leap_second() {
        let mut msf = MSFUtils::default();
        msf.second = 60;
        for b in 0..=16 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        // insert the positive leap second, left None on purpose (it should not affect decoding)
        msf.bit_buffer_a[17] = None;
        msf.bit_buffer_b[17] = None;
        for b in 17..=59 {
            msf.bit_buffer_a[b + 1] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b + 1] = Some(BIT_BUFFER_B[b]);
        }
        assert_eq!(msf.end_of_minute_marker_present(), true);
        assert_eq!(msf.get_minute_length(), msf.second + 1);
        msf.decode_time(false);
        // we should have a valid decoding:
        assert_eq!(msf.radio_datetime.get_minute(), Some(58));
        assert_eq!(msf.radio_datetime.get_hour(), Some(14));
        assert_eq!(msf.radio_datetime.get_weekday(), Some(6));
        assert_eq!(msf.radio_datetime.get_day(), Some(23));
        assert_eq!(msf.radio_datetime.get_month(), Some(10));
        assert_eq!(msf.radio_datetime.get_year(), Some(22));
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, Some(true));
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(true));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        );
        assert_eq!(msf.radio_datetime.get_leap_second(), None); // not available
        assert_eq!(msf.dut1, Some(-2));
        assert_eq!(msf.first_minute, false);
    }
    #[test]
    fn test_decode_time_complete_minute_bad_bits() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        assert_eq!(msf.get_minute_length(), msf.second + 1); // EOM marker absent
        for b in 0..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        // introduce some distortions:
        msf.bit_buffer_b[1] = Some(true); // now both 1-8 and 9-16 are positive, which is an error
        msf.bit_buffer_a[31] = None; // None hour
        msf.bit_buffer_a[48] = Some(!msf.bit_buffer_a[48].unwrap());
        msf.decode_time(false);
        assert_eq!(msf.radio_datetime.get_minute(), None); // bad parity and first decoding
        assert_eq!(msf.radio_datetime.get_hour(), None); // bad parity and first decoding
        assert_eq!(msf.radio_datetime.get_weekday(), Some(6));
        assert_eq!(msf.radio_datetime.get_day(), None); // broken bit
        assert_eq!(msf.radio_datetime.get_month(), None); // broken parity and first decoding
        assert_eq!(msf.radio_datetime.get_year(), Some(22));
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, None); // broken bit
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(false)); // bad parity
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        );
        assert_eq!(msf.radio_datetime.get_leap_second(), None);
        assert_eq!(msf.dut1, None);
    }
    #[test]
    fn continue_decode_time_complete_minute_jumped_values() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        assert_eq!(msf.get_minute_length(), msf.second + 1); // EOM marker absent
        for b in 0..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        msf.decode_time(false);
        assert_eq!(msf.radio_datetime.get_minute(), Some(58));
        assert_eq!(msf.radio_datetime.get_jump_minute(), false);
        assert_eq!(msf.first_minute, false);
        // minute 58 is really cool, so do not update bit 51 (and 57)
        msf.decode_time(false);
        assert_eq!(msf.radio_datetime.get_minute(), Some(58));
        assert_eq!(msf.radio_datetime.get_hour(), Some(14));
        assert_eq!(msf.radio_datetime.get_weekday(), Some(6));
        assert_eq!(msf.radio_datetime.get_day(), Some(23));
        assert_eq!(msf.radio_datetime.get_month(), Some(10));
        assert_eq!(msf.radio_datetime.get_year(), Some(22));
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, Some(true));
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(true));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        );
        assert_eq!(msf.radio_datetime.get_leap_second(), None);
        assert_eq!(msf.radio_datetime.get_jump_minute(), true);
        assert_eq!(msf.radio_datetime.get_jump_hour(), false);
        assert_eq!(msf.radio_datetime.get_jump_weekday(), false);
        assert_eq!(msf.radio_datetime.get_jump_day(), false);
        assert_eq!(msf.radio_datetime.get_jump_month(), false);
        assert_eq!(msf.radio_datetime.get_jump_year(), false);
    }
    #[test]
    fn continue_decode_time_complete_minute_bad_bits() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        assert_eq!(msf.get_minute_length(), msf.second + 1); // EOM marker absent
        for b in 0..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        msf.decode_time(false);
        assert_eq!(msf.first_minute, false);
        // update for the next minute:
        msf.bit_buffer_a[51] = Some(true);
        msf.bit_buffer_b[57] = Some(true);
        // introduce some distortions:
        msf.bit_buffer_a[31] = None; // None hour
        msf.bit_buffer_a[48] = Some(!msf.bit_buffer_a[48].unwrap());
        msf.decode_time(false);
        assert_eq!(msf.radio_datetime.get_minute(), Some(59)); // bad parity
        assert_eq!(msf.radio_datetime.get_hour(), Some(14));
        assert_eq!(msf.radio_datetime.get_weekday(), Some(6)); // broken parity
        assert_eq!(msf.radio_datetime.get_day(), Some(23)); // broken bit
        assert_eq!(msf.radio_datetime.get_month(), Some(10)); // broken parity
        assert_eq!(msf.radio_datetime.get_year(), Some(22)); // broken parity
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, None); // broken bit
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(false)); // bad parity
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        );
        assert_eq!(msf.radio_datetime.get_leap_second(), None);
        assert_eq!(msf.radio_datetime.get_jump_minute(), false);
        assert_eq!(msf.radio_datetime.get_jump_hour(), false);
        assert_eq!(msf.radio_datetime.get_jump_weekday(), false);
        assert_eq!(msf.radio_datetime.get_jump_day(), false);
        assert_eq!(msf.radio_datetime.get_jump_month(), false);
        assert_eq!(msf.radio_datetime.get_jump_year(), false);
    }
    #[test]
    fn continue_decode_time_complete_minute_dst_change_to_winter() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        for b in 0..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        // DST change must be at top of hour and
        // announcements only count before the hour, so set minute to 59:
        msf.bit_buffer_a[51] = Some(true);
        msf.bit_buffer_b[57] = Some(true);
        // announce a DST change:
        msf.bit_buffer_b[53] = Some(true);
        msf.decode_time(false);
        assert_eq!(msf.radio_datetime.get_minute(), Some(59));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_ANNOUNCED | radio_datetime_utils::DST_SUMMER)
        );
        // next minute and hour:
        msf.bit_buffer_a[45] = Some(false);
        msf.bit_buffer_a[47] = Some(false);
        msf.bit_buffer_a[48] = Some(false);
        msf.bit_buffer_a[51] = Some(false);
        msf.bit_buffer_a[44] = Some(true);
        msf.bit_buffer_b[57] = Some(false);
        // which will have a DST change:
        msf.bit_buffer_b[53] = Some(true);
        msf.bit_buffer_b[58] = Some(false);
        msf.decode_time(false);
        assert_eq!(msf.radio_datetime.get_minute(), Some(0));
        assert_eq!(msf.radio_datetime.get_hour(), Some(15));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_PROCESSED)
        ); // DST flipped off
    }

    // strict checks
    #[test]
    fn test_decode_time_incomplete_minute_strict() {
        let mut msf = MSFUtils::default();
        assert_eq!(msf.first_minute, true);
        msf.second = 42;
        // note that msf.bit_buffer_[ab] are still empty
        assert_ne!(msf.get_minute_length(), msf.second);
        assert_eq!(msf.parity_1, None);
        msf.decode_time(true);
        // not enough seconds in this minute, so nothing should happen:
        assert_eq!(msf.parity_1, None);
    }
    #[test]
    fn test_decode_time_complete_minute_ok_strict() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        assert_eq!(msf.get_minute_length(), msf.second + 1); // EOM marker absent
        for b in 0..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        assert_eq!(msf.end_of_minute_marker_present(), true);
        msf.decode_time(true);
        // we should have a valid decoding:
        assert_eq!(msf.radio_datetime.get_minute(), Some(58));
        assert_eq!(msf.radio_datetime.get_hour(), Some(14));
        assert_eq!(msf.radio_datetime.get_weekday(), Some(6));
        assert_eq!(msf.radio_datetime.get_day(), Some(23));
        assert_eq!(msf.radio_datetime.get_month(), Some(10));
        assert_eq!(msf.radio_datetime.get_year(), Some(22));
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, Some(true));
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(true));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        );
        assert_eq!(msf.radio_datetime.get_leap_second(), None); // not available
        assert_eq!(msf.dut1, Some(-2));
    }
    #[test]
    fn test_decode_time_complete_minute_ok_negative_leap_second_strict() {
        let mut msf = MSFUtils::default();
        msf.second = 58;
        for b in 0..=15 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        // bit 16 removed
        for b in 17..=59 {
            msf.bit_buffer_a[b - 1] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b - 1] = Some(BIT_BUFFER_B[b]);
        }
        assert_eq!(msf.end_of_minute_marker_present(), true);
        assert_eq!(msf.get_minute_length(), msf.second + 1);
        msf.decode_time(true);
        // we should have a valid decoding:
        assert_eq!(msf.radio_datetime.get_minute(), Some(58));
        assert_eq!(msf.radio_datetime.get_hour(), Some(14));
        assert_eq!(msf.radio_datetime.get_weekday(), Some(6));
        assert_eq!(msf.radio_datetime.get_day(), Some(23));
        assert_eq!(msf.radio_datetime.get_month(), Some(10));
        assert_eq!(msf.radio_datetime.get_year(), Some(22));
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, Some(true));
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(true));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        );
        assert_eq!(msf.radio_datetime.get_leap_second(), None); // not available
        assert_eq!(msf.dut1, Some(-2));
        assert_eq!(msf.first_minute, false);
    }
    #[test]
    fn test_decode_time_complete_minute_ok_positive_leap_second_strict() {
        let mut msf = MSFUtils::default();
        msf.second = 60;
        for b in 0..=16 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        // insert the positive leap second, left None on purpose (it should not affect decoding)
        msf.bit_buffer_a[17] = None;
        msf.bit_buffer_b[17] = None;
        for b in 17..=59 {
            msf.bit_buffer_a[b + 1] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b + 1] = Some(BIT_BUFFER_B[b]);
        }
        assert_eq!(msf.end_of_minute_marker_present(), true);
        assert_eq!(msf.get_minute_length(), msf.second + 1);
        msf.decode_time(true);
        // we should have a valid decoding:
        assert_eq!(msf.radio_datetime.get_minute(), Some(58));
        assert_eq!(msf.radio_datetime.get_hour(), Some(14));
        assert_eq!(msf.radio_datetime.get_weekday(), Some(6));
        assert_eq!(msf.radio_datetime.get_day(), Some(23));
        assert_eq!(msf.radio_datetime.get_month(), Some(10));
        assert_eq!(msf.radio_datetime.get_year(), Some(22));
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, Some(true));
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(true));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        );
        assert_eq!(msf.radio_datetime.get_leap_second(), None); // not available
        assert_eq!(msf.dut1, Some(-2));
        assert_eq!(msf.first_minute, false);
    }
    #[test]
    fn test_decode_time_complete_minute_bad_bits_strict() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        assert_eq!(msf.get_minute_length(), msf.second + 1); // EOM marker absent
        for b in 0..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        // introduce some distortions:
        msf.bit_buffer_b[1] = Some(true); // now both 1-8 and 9-16 are positive, which is an error
        msf.bit_buffer_a[31] = None; // None hour
        msf.bit_buffer_a[48] = Some(!msf.bit_buffer_a[48].unwrap());
        msf.decode_time(true);
        assert_eq!(msf.radio_datetime.get_minute(), None); // bad parity and first decoding
        assert_eq!(msf.radio_datetime.get_hour(), None); // bad parity and first decoding
        assert_eq!(msf.radio_datetime.get_weekday(), None); // strict check failed
        assert_eq!(msf.radio_datetime.get_day(), None); // broken bit
        assert_eq!(msf.radio_datetime.get_month(), None); // broken parity and first decoding
        assert_eq!(msf.radio_datetime.get_year(), None); // strict check failed
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, None); // broken bit
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(false)); // bad parity
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        ); // not affected by strict checking
        assert_eq!(msf.radio_datetime.get_leap_second(), None);
        assert_eq!(msf.dut1, None);
    }
    #[test]
    fn continue_decode_time_complete_minute_jumped_values_strict() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        assert_eq!(msf.get_minute_length(), msf.second + 1); // EOM marker absent
        for b in 0..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        msf.decode_time(true);
        assert_eq!(msf.radio_datetime.get_minute(), Some(58));
        assert_eq!(msf.radio_datetime.get_jump_minute(), false);
        assert_eq!(msf.first_minute, false);
        // minute 58 is really cool, so do not update bit 51 (and 57)
        msf.decode_time(true);
        assert_eq!(msf.radio_datetime.get_minute(), Some(58));
        assert_eq!(msf.radio_datetime.get_hour(), Some(14));
        assert_eq!(msf.radio_datetime.get_weekday(), Some(6));
        assert_eq!(msf.radio_datetime.get_day(), Some(23));
        assert_eq!(msf.radio_datetime.get_month(), Some(10));
        assert_eq!(msf.radio_datetime.get_year(), Some(22));
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, Some(true));
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(true));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        );
        assert_eq!(msf.radio_datetime.get_leap_second(), None);
        assert_eq!(msf.radio_datetime.get_jump_minute(), true);
        assert_eq!(msf.radio_datetime.get_jump_hour(), false);
        assert_eq!(msf.radio_datetime.get_jump_weekday(), false);
        assert_eq!(msf.radio_datetime.get_jump_day(), false);
        assert_eq!(msf.radio_datetime.get_jump_month(), false);
        assert_eq!(msf.radio_datetime.get_jump_year(), false);
    }
    #[test]
    fn continue_decode_time_complete_minute_bad_bits_strict() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        assert_eq!(msf.get_minute_length(), msf.second + 1); // EOM marker absent
        for b in 0..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        msf.decode_time(true);
        assert_eq!(msf.first_minute, false);
        // update for the next minute:
        msf.bit_buffer_a[51] = Some(true);
        msf.bit_buffer_b[57] = Some(true);
        // introduce some distortions:
        msf.bit_buffer_a[31] = None; // None hour
        msf.bit_buffer_a[48] = Some(!msf.bit_buffer_a[48].unwrap());
        msf.decode_time(true);
        assert_eq!(msf.radio_datetime.get_minute(), Some(59)); // bad parity
        assert_eq!(msf.radio_datetime.get_hour(), Some(14));
        assert_eq!(msf.radio_datetime.get_weekday(), Some(6)); // broken parity
        assert_eq!(msf.radio_datetime.get_day(), Some(23)); // broken bit
        assert_eq!(msf.radio_datetime.get_month(), Some(10)); // broken parity
        assert_eq!(msf.radio_datetime.get_year(), Some(22)); // broken parity
        assert_eq!(msf.parity_1, Some(true));
        assert_eq!(msf.parity_2, None); // broken bit
        assert_eq!(msf.parity_3, Some(true));
        assert_eq!(msf.parity_4, Some(false)); // bad parity
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_SUMMER)
        );
        assert_eq!(msf.radio_datetime.get_leap_second(), None);
        assert_eq!(msf.radio_datetime.get_jump_minute(), false);
        assert_eq!(msf.radio_datetime.get_jump_hour(), false);
        assert_eq!(msf.radio_datetime.get_jump_weekday(), false);
        assert_eq!(msf.radio_datetime.get_jump_day(), false);
        assert_eq!(msf.radio_datetime.get_jump_month(), false);
        assert_eq!(msf.radio_datetime.get_jump_year(), false);
    }
    #[test]
    fn continue_decode_time_complete_minute_dst_change_to_summer_strict() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        for b in 0..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
            msf.bit_buffer_b[b] = Some(BIT_BUFFER_B[b]);
        }
        // flip to winter
        msf.bit_buffer_b[58] = Some(false);
        // DST change must be at top of hour and
        // announcements only count before the hour, so set minute to 59:
        msf.bit_buffer_a[51] = Some(true);
        msf.bit_buffer_b[57] = Some(true);
        // announce a DST change:
        msf.bit_buffer_b[53] = Some(true);
        msf.decode_time(true);
        assert_eq!(msf.radio_datetime.get_minute(), Some(59));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_ANNOUNCED)
        );
        // next minute and hour:
        msf.bit_buffer_a[45] = Some(false);
        msf.bit_buffer_a[47] = Some(false);
        msf.bit_buffer_a[48] = Some(false);
        msf.bit_buffer_a[51] = Some(false);
        msf.bit_buffer_a[44] = Some(true);
        msf.bit_buffer_b[57] = Some(false);
        // which will have a DST change:
        msf.bit_buffer_b[53] = Some(true);
        msf.bit_buffer_b[58] = Some(true);
        msf.decode_time(true);
        assert_eq!(msf.radio_datetime.get_minute(), Some(0));
        assert_eq!(msf.radio_datetime.get_hour(), Some(15));
        assert_eq!(
            msf.radio_datetime.get_dst(),
            Some(radio_datetime_utils::DST_PROCESSED | radio_datetime_utils::DST_SUMMER)
        ); // DST flipped on
    }

    #[test]
    fn test_increase_second_same_minute_ok() {
        let mut msf = MSFUtils::default();
        msf.second = 37;
        assert_eq!(msf.end_of_minute_marker_present(), false);
        // all date/time values are None
        assert_eq!(msf.increase_second(), true);
        assert_eq!(msf.end_of_minute_marker_present(), false);
        assert_eq!(msf.first_minute, true);
        assert_eq!(msf.second, 38);
    }
    #[test]
    fn test_increase_second_same_minute_overflow() {
        let mut msf = MSFUtils::default();
        msf.second = 59;
        // leap second value is None, or 0111_1110 is "in the middle"
        assert_eq!(msf.end_of_minute_marker_present(), false);
        assert_eq!(msf.increase_second(), false);
        assert_eq!(msf.end_of_minute_marker_present(), false);
        assert_eq!(msf.first_minute, true);
        assert_eq!(msf.second, 0);
    }
    #[test]
    fn test_increase_second_new_minute_ok() {
        let mut msf = MSFUtils::default();
        msf.new_minute = true;
        msf.second = 59;
        assert_eq!(msf.get_minute_length(), msf.second + 1); // EOM marker absent
        for b in 52..=59 {
            msf.bit_buffer_a[b] = Some(BIT_BUFFER_A[b]);
        }
        assert_eq!(msf.end_of_minute_marker_present(), true);
        assert_eq!(msf.increase_second(), true);
        assert_eq!(msf.end_of_minute_marker_present(), false);
        assert_eq!(msf.first_minute, true);
        assert_eq!(msf.second, 0);
    }
    #[test]
    fn test_increase_second_new_minute_none_values() {
        let mut msf = MSFUtils::default();
        msf.new_minute = true;
        msf.second = 59;
        assert_eq!(msf.end_of_minute_marker_present(), false);
        // all date/time values left None
        assert_eq!(msf.increase_second(), true);
        assert_eq!(msf.end_of_minute_marker_present(), false);
        assert_eq!(msf.first_minute, true);
        assert_eq!(msf.second, 0);
    }
}
