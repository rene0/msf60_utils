//! NPL receiver for embedded platforms using e.g. a Canaduino V2 receiver.

#![no_std]

use radio_datetime_utils::RadioDateTimeUtils;

/// Time in microseconds for a bit to be considered 1
const ACTIVE_LIMIT: u32 = 150_000;
/// Minimum amount of time in microseconds between two bits, mostly to deal with noise
const SECOND_LIMIT: u32 = 950_000;
#[allow(dead_code)]
/// Time in microseconds for the minute marker to be detected
const MINUTE_LIMIT: u32 = 450_000;
/// Signal is considered lost after this many microseconds
const PASSIVE_LIMIT: u32 = 1_500_000;

/// NPL decoder class
pub struct NPLUtils {
    before_first_edge: bool,
    first_minute: bool,
    new_minute: bool,
    act_len: u32,
    sec_len: u32,
    split_second: bool,
    second: u8,
    bit_buffer_a: [Option<bool>; 60],
    bit_buffer_b: [Option<bool>; 60],
    radio_datetime: RadioDateTimeUtils,
    parity_1: Option<bool>,
    parity_2: Option<bool>,
    parity_3: Option<bool>,
    parity_4: Option<bool>,
    frame_counter: u8,
    ticks_per_second: u8,
    ind_time: bool,
    ind_bit_a: bool,
    ind_bit_b: bool,
    ind_error: bool,
}

impl NPLUtils {
    pub fn new(tps: u8) -> Self {
        Self {
            before_first_edge: true,
            first_minute: true,
            new_minute: false,
            act_len: 0,
            sec_len: 0,
            second: 0,
            split_second: false,
            bit_buffer_a: [None; 60],
            bit_buffer_b: [None; 60],
            radio_datetime: RadioDateTimeUtils::new(0),
            parity_1: None,
            parity_2: None,
            parity_3: None,
            parity_4: None,
            frame_counter: 0,
            ticks_per_second: tps,
            ind_time: true,
            ind_bit_a: false,
            ind_bit_b: false,
            ind_error: true,
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

    /// Get the second counter.
    pub fn get_second(&self) -> u8 {
        self.second
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

    /// Get the frame-in-second counter.
    pub fn get_frame_counter(&self) -> u8 {
        self.frame_counter
    }

    /// Return if the time (i.e. new second or minute) indicator is active.
    pub fn get_ind_time(&self) -> bool {
        self.ind_time
    }

    /// Return if the currently received bit A is a 1.
    pub fn get_ind_bit_a(&self) -> bool {
        self.ind_bit_a
    }

    /// Return if the currently received bit B is a 1.
    pub fn get_ind_bit_b(&self) -> bool {
        self.ind_bit_b
    }

    /// Return if there was an error receiving this bit.
    pub fn get_ind_error(&self) -> bool {
        self.ind_error
    }

    /**
     * Determine the bit value if a new edge is received. indicates reception errors,
     * and checks if a new minute has started.
     *
     * # Arguments
     * * `is_low_edge` - indicates that the edge has gone from high to low (as opposed to
     *                   low-to-high).
     * * `t0` - time stamp of the previously received edge, in microseconds
     * * `t1` - time stamp of the currently received edge, in microseconds
     */
    // FIXME this code is to be tested and will *not* work properly in its current state.
    pub fn handle_new_edge(&mut self, is_low_edge: bool, t0: u32, t1: u32) {
        if self.before_first_edge {
            self.before_first_edge = false;
            return;
        }
        let t_diff = radio_datetime_utils::time_diff(t0, t1);
        self.sec_len += t_diff;
        if is_low_edge {
            self.bit_buffer_a[self.second as usize] = Some(false);
            self.bit_buffer_b[self.second as usize] = Some(false);
            /*
                       if self.frame_counter < 4 * self.ticks_per_second / 10 {
                           // suppress noise in case a bit got split
                           self.act_len += t_diff;
                       }
            */
            if self.act_len > ACTIVE_LIMIT {
                self.ind_bit_a = true;
                self.bit_buffer_a[self.second as usize] = Some(true);
                if self.act_len > 2 * ACTIVE_LIMIT {
                    self.ind_error = true;
                    self.bit_buffer_a[self.second as usize] = None;
                }
            }
        } else if self.sec_len > PASSIVE_LIMIT {
            self.ind_error = true;
            self.act_len = 0;
            self.sec_len = 0;
        } else if self.sec_len > SECOND_LIMIT {
            self.ind_time = true;
            // self.new_minute = self.sec_len > MINUTE_LIMIT; // TODO
            self.act_len = 0;
            self.sec_len = 0;
            if !self.split_second {
                self.frame_counter = 0;
            }
            self.split_second = false;
        } else {
            self.split_second = true;
            // self.bit_buffer_a[self.second as usize] = None; // perhaps?
            // self.bit_buffer_b[self.second as usize] = None; // perhaps?
            self.ind_error = true;
        }
    }

    /// Determine the length of this minute in bits.
    pub fn get_minute_length(&self) -> u8 {
        59 // TODO determine position of 0111_1110 end-of-minute marker and consequently add -1, 0, 1
    }

    /// Increase or reset `second` and clear `first_minute` when appropriate.
    pub fn increase_second(&mut self) {
        if self.new_minute {
            if self.first_minute
                && self.second == self.get_minute_length()
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
            if self.second == self.get_minute_length() + 1 {
                self.second = 0;
            }
        }
    }

    /// Update the frame counter and the status of the time, bit, and error indicators when a
    /// new timer tick arrives. Calculate the current date ane time upon a new minute.
    pub fn handle_new_timer_tick(&mut self) {
        if self.frame_counter == 0 {
            self.ind_time = true;
            self.ind_bit_a = false;
            self.ind_bit_b = false;
            self.ind_error = false;
            if self.new_minute {
                self.decode_time();
            }
        } else if (self.frame_counter == self.ticks_per_second / 10 && !self.new_minute)
            || (self.frame_counter == 7 * self.ticks_per_second / 10 && self.new_minute)
        {
            self.ind_time = false;
        }
        if self.frame_counter == self.ticks_per_second {
            self.frame_counter = 0;
        } else {
            self.frame_counter += 1;
        }
    }

    /// Decode the time broadcast during the last minute, tolerate bad DST status.
    fn decode_time(&mut self) {
        if !self.first_minute {
            self.radio_datetime.add_minute();
        }
        if self.second == self.get_minute_length() {
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
