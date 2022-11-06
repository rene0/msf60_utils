//! NPL receiver, e.g. for embedded platforms.

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
    bit_buffer_a: [Option<bool>; 60],
    bit_buffer_b: [Option<bool>; 60],
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
            bit_buffer_a: [None; 60],
            bit_buffer_b: [None; 60],
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
     * This function can deal with spikes, which are arbitrarily set to SPIKE_LIMIT milliseconds.
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
    pub fn get_this_minute_length(&self) -> u8 {
        59 // TODO determine position of 0111_1110 end-of-minute marker and consequently add -1, 0, 1
    }

    /// Increase or reset `second` and clear `first_minute` when appropriate.
    pub fn increase_second(&mut self) {
        if self.new_minute {
            if self.first_minute
                && self.second == self.get_this_minute_length()
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
            // wrap in case we missed the minute marker to prevent index-out-of-range
            self.second += 1;
            if self.second == self.get_this_minute_length() + 1 {
                self.second = 0;
            }
        }
    }

    /// Decode the time broadcast during the last minute, tolerate bad DST status.
    pub fn decode_time(&mut self) {
        if !self.first_minute {
            self.radio_datetime.add_minute();
        }
        if self.second == self.get_this_minute_length() {
            self.parity_1 =
                radio_datetime_utils::get_parity(&self.bit_buffer_a, 17, 24, self.bit_buffer_b[54]);
            let tmp0 = radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 24, 17);
            self.radio_datetime
                .set_year(tmp0, self.parity_1 == Some(true), !self.first_minute);

            self.parity_2 =
                radio_datetime_utils::get_parity(&self.bit_buffer_a, 25, 35, self.bit_buffer_b[55]);
            let tmp0 = radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 29, 25);
            self.radio_datetime
                .set_month(tmp0, self.parity_2 == Some(true), !self.first_minute);
            let tmp2 = radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 35, 30); // day, delayed assignment

            self.parity_3 =
                radio_datetime_utils::get_parity(&self.bit_buffer_a, 36, 38, self.bit_buffer_b[56]);
            let tmp0 = radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 38, 36);
            self.radio_datetime
                .set_weekday(tmp0, self.parity_3 == Some(true), !self.first_minute);
            self.radio_datetime.set_day(
                tmp2,
                self.parity_1 == Some(true)
                    && self.parity_2 == Some(true)
                    && self.parity_3 == Some(true),
                !self.first_minute,
            );

            self.parity_4 =
                radio_datetime_utils::get_parity(&self.bit_buffer_a, 39, 51, self.bit_buffer_b[57]);
            let tmp0 = radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 44, 39);
            self.radio_datetime
                .set_hour(tmp0, self.parity_4 == Some(true), !self.first_minute);
            let tmp0 = radio_datetime_utils::get_bcd_value(&self.bit_buffer_a, 51, 45);
            self.radio_datetime
                .set_minute(tmp0, self.parity_4 == Some(true), !self.first_minute);
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

    #[test]
    fn test_new_edge() {
        // TODO implement
    }

    #[test]
    fn test_decode_time() {
        // TODO implement
    }

    #[test]
    fn test_increase_second() {
        // TODO implement
    }
}
