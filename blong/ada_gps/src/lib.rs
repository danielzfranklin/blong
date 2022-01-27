#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod cmd;
mod integer_percent;
mod locus;
mod log_macros;

pub use cmd::parse::Error as ParseError;
pub use integer_percent::IntegerPercent;
pub use locus::logged_point::{Error as ParseLoggedPointError, LoggedPoint};
pub use locus::status::LoggerStatus;

use alloc::vec::Vec;
use bbqueue::BBBuffer;
use defmt::Format;
use embedded_hal::{blocking::delay::DelayUs, serial};
use lexical_core::FormattedSize;

// NOTE: See PMTK_A11-datasheet.pdf

// TODO: Avoid allocating

// TODO: Figure out what to divide ticks by to have it be consistent across clock? speeds
const RX_BUF_SIZE: usize = 1024;
const MAX_CMD_TRIES: usize = 5;
const MAX_CMD_TRIES_WITHOUT_NMEA_DISABLED: usize = 20;
const MAX_READ_CMD_US: u32 = 500_000;
const MAX_WRITE_CMD_US: u32 = 50_000;
const DELAY_BEFORE_RETRY_US: u32 = 80_000;
const MAX_READ_ERRORS_ON_BOOT: usize = 50;
/// Maximum number of undocumented packets before we get the documented boot
/// indicator packets.
const MAX_READ_SPURIOUS_BEFORE_BOOT: usize = 1_000;
// This helps us avoid some spurious messages
const WAIT_BEFORE_CHECKING_BOOT_READY_US: u32 = 50_000;
/// Maximum number of undocumented packets after we get the documented boot
/// indicator packets.
const MAX_READ_SPURIOUS_AFTER_BOOT_READY: usize = 20;
// max 24 chunks, in basic mode one point is 2 chunks
const MAX_POINTS_PER_LOCUS_DATA_PACKET: usize = 12;

pub type RxBuf = BBBuffer<{ RX_BUF_SIZE }>;
pub type RxProducer<'rx> = bbqueue::Producer<'rx, { RX_BUF_SIZE }>;
pub type RxConsumer<'rx> = bbqueue::Consumer<'rx, { RX_BUF_SIZE }>;

pub struct Gps<'rx, Tx, Delay> {
    disabled_nmea_output: bool,
    rx: RxConsumer<'rx>,
    tx: Tx,
    delay: Delay,
}

impl<'rx, Tx, Delay> Gps<'rx, Tx, Delay>
where
    Tx: serial::Write<u8>,
    Delay: DelayUs<u32>,
{
    pub fn new(
        rx: RxConsumer<'rx>,
        tx: Tx,
        delay: Delay,
        already_disabled_nmea_output: bool,
    ) -> Self {
        Self {
            disabled_nmea_output: already_disabled_nmea_output,
            rx,
            tx,
            delay,
        }
    }

    pub fn configure_logger_interval(&mut self, secs: u32) -> Result<(), Error<Tx::Error>> {
        // PMTK_LOCUS_CONFIG
        let mut buf = [0_u8; u32::FORMATTED_SIZE_DECIMAL];
        let secs_ascii = u32_to_base10_ascii(secs, &mut buf);
        self.send_mtk_cmd(b"187", &[b"1", &secs_ascii])
    }

    pub fn erase_logs(&mut self) -> Result<(), Error<Tx::Error>> {
        // PMTK_LOCUS_ERASE_FLASH
        info!("Erasing logs");
        self.send_mtk_cmd(b"184", &[b"1"])
    }

    pub fn start_logging(&mut self) -> Result<(), Error<Tx::Error>> {
        // PMTK_LOCUS_STOP_LOGGER, 0 = start
        info!("Starting logging");
        self.send_mtk_cmd(b"185", &[b"0"])
    }

    pub fn stop_logging(&mut self) -> Result<(), Error<Tx::Error>> {
        // PMTK_LOCUS_STOP_LOGGER, 1 = stop
        info!("Stopping logging");
        self.send_mtk_cmd(b"185", &[b"1"])
    }

    pub fn logger_status(&mut self) -> Result<LoggerStatus, Error<Tx::Error>> {
        // PMTK_LOCUS_QUERY_STATUS
        // Interval mode: 8 (1 << 3)
        info!("Querying logger status");

        let fields = self.send_mtk_cmd_for_reply(b"183", &[], b"LOG", 10)?;

        let serial_field = &fields[0];
        let logging_type_field = &fields[1];
        let mode_field = &fields[2];
        let content_field = &fields[3];
        let interval_field = &fields[4];
        let distance_field = &fields[5];
        let speed_field = &fields[6];
        let status_field = &fields[7];
        let number_field = &fields[8];
        let percent_field = &fields[9];

        debug!(
            "Raw status fields: serial={=[u8]:a}, logging_type={=[u8]:a}, mode={=[u8]:a}, content={=[u8]:a}, interval={=[u8]:a}, distance={=[u8]:a}, speed={=[u8]:a}, status={=[u8]:a}, number={=[u8]:a}, percent={=[u8]:a}",
            serial_field,
            logging_type_field,
            mode_field,
            content_field,
            interval_field,
            distance_field,
            speed_field,
            status_field,
            number_field,
            percent_field,
        );

        let status = LoggerStatus {
            interval: cmd::parse::integer_field(interval_field)?,
            is_on: cmd::parse::bool_field(status_field, b"0", b"1")?,
            record_count: cmd::parse::integer_field(number_field)?,
            percent_full: cmd::parse::integer_percent_field(percent_field)?,
        };

        info!("Got logger status: {:?}", &status);

        Ok(status)
    }

    pub fn read_logs<F>(&mut self, mut on_point: F) -> Result<(), Error<Tx::Error>>
    where
        F: FnMut(usize, LoggedPoint) -> (),
    {
        info!("Reading logs");

        // NOTE: We don't retry because this is super expensive.

        self.ensure_nmea_output_disabled()?;

        // PMTK_Q_LOCUS_DATA, 0 = full
        //  I can't figure out how partial dumps work.
        self.write_cmd_raw(b"PMTK622", &[b"0"])?;

        let locus_start = self.read_reply_raw(b"PMTKLOX", 2)?;
        if locus_start[0] != b"0" {
            error!("Expected LOCUS start packet");
            return Err(Error::Protocol);
        }
        let packet_count: usize = cmd::parse::integer_field(&locus_start[1])?
            .try_into()
            .unwrap();
        let point_count_estimate = packet_count * MAX_POINTS_PER_LOCUS_DATA_PACKET;

        for n in 0..packet_count {
            let locus_data = self.read_reply_raw(b"PMTKLOX", 2)?;

            if locus_data[0] != b"1" {
                error!("Expected LOCUS data packet");
                return Err(Error::Protocol);
            }

            let actual_n: usize = cmd::parse::integer_field(&locus_data[1])?
                .try_into()
                .unwrap();
            if actual_n != n {
                error!(
                    "Expected LOCUS data packet number {}, got number {}",
                    n, actual_n
                );
                return Err(Error::Protocol);
            }

            locus::logged_point::parse_data_fields(&locus_data[2..], |point| {
                on_point(point_count_estimate, point)
            })?;
        }

        let locus_end = self.read_reply_raw(b"PMTKLOX", 2)?;
        if locus_end[0] != b"2" {
            error!("Expected LOCUS end packet");
            return Err(Error::Protocol);
        }

        Ok(())
    }

    /// Restart keeping all saved data.
    pub fn hot_restart(&mut self) -> Result<(), Error<Tx::Error>> {
        // PMTK_CMD_HOT_START
        info!("Hot restarting");
        self.send_reboot_cmd(b"PMTK101")
    }

    /// Restart keeping everything but ephemeris.
    pub fn warm_restart(&mut self) -> Result<(), Error<Tx::Error>> {
        // PMTK_CMD_WARM_START
        info!("Warm restarting");
        self.send_reboot_cmd(b"PMTK102")
    }

    /// Restart keeping everything but time, position, almanacs and ephemeris.
    pub fn cold_restart(&mut self) -> Result<(), Error<Tx::Error>> {
        // PMTK_CMD_COLD_START
        info!("Cold restarting");
        self.send_reboot_cmd(b"PMTK103")
    }

    /// Restart, clearing everything.
    ///
    /// It's essentially a cold restart, but additionally clear system/user
    /// configurations at re-start.
    pub fn factory_reset(&mut self) -> Result<(), Error<Tx::Error>> {
        // PMTK_CMD_FULL_COLD_START
        info!("Factory resetting");
        self.send_reboot_cmd(b"PMTK104")
    }

    fn send_reboot_cmd(&mut self, cmd: &[u8]) -> Result<(), Error<Tx::Error>> {
        self.with_retries(MAX_CMD_TRIES, |gps| {
            gps.disabled_nmea_output = false;
            gps.write_cmd_raw(cmd, &[])?;
            gps.wait_for_boot()?;
            gps.ensure_nmea_output_disabled()?;
            Ok(())
        })
        .map(|(tries, ())| {
            debug!("Took {} tries to reboot with {=[u8]:a}", tries, cmd);
        })
        .map_err(|(tries, err)| {
            error!("Failed to reboot with {=[u8]:a} after {} tries", cmd, tries);
            err
        })
    }

    fn wait_for_boot(&mut self) -> Result<(), Error<Tx::Error>> {
        // PMTK_A11.pdf:
        //   In addition, when the GPS module is powered-on or restarted via
        //   command, both "$PMTK010,001*2E<CR><LF>" and
        //   $PMTK011,MTKGPS*08<CR><LF>" will be returned at the same time after
        //   GPS engine has successfully completed boot-up stage.

        let mut seen_boot_sys_msg = false;
        let mut seen_mtkgps = false;
        let mut read_errors = 0;
        let mut read_spurious = 0;
        loop {
            if seen_boot_sys_msg && seen_mtkgps {
                info!("Booted");
                break;
            }

            if read_errors > MAX_READ_ERRORS_ON_BOOT {
                error!("Exceeded MAX_READ_ERRORS_ON_BOOT");
                return Err(Error::BootFailed);
            }

            if read_spurious > MAX_READ_SPURIOUS_BEFORE_BOOT {
                error!("Exceeded MAX_READ_SPURIOUS_ON_BOOT");
                return Err(Error::BootFailed);
            }

            match self.read_cmd_raw() {
                Ok((name, fields)) => {
                    if name == b"PMTK010" && fields == &[b"001"] {
                        debug!("Saw boot sys msg");
                        seen_boot_sys_msg = true;
                    } else if name == b"PMTK011" && fields == &[b"MTKGPS"] {
                        debug!("Saw boot mtkgps");
                        seen_mtkgps = true;
                    } else {
                        debug!("Read spurious on boot: {=[u8]:a}", name);
                        read_spurious += 1;
                    }
                }
                Err(_) => {
                    debug!("Read error while waiting for boot");
                    read_errors += 1;
                }
            }
        }

        self.delay_us(WAIT_BEFORE_CHECKING_BOOT_READY_US);
        self.flush_rx_queue();

        // This has the beneficial side-effect of failing and retrying until
        // we've gone through the undocumented boot messages, so we don't get
        // those when we're expecting replies later.
        //
        // Because of those extra messages, we give ourselves extra tries here.
        self.check_ready(MAX_READ_SPURIOUS_AFTER_BOOT_READY)?;

        Ok(())
    }

    /// Check the gps is ready to receive commands.
    ///
    /// We do this by sending a cheap command and reading the reply.
    ///
    /// That means this is relatively expensive and so should only be used after
    /// boot or before sending an especially long command we don't want to
    /// retry.
    ///
    /// For cheap commands we may as well just retry the command itself.
    fn check_ready(&mut self, max_tries: usize) -> Result<(), Error<Tx::Error>> {
        self.with_retries(max_tries, |gps| {
            // PMTK_Q_RELEASE
            gps.write_cmd_raw(b"PMTK605", &[])?;

            // PMTK_DT_RELEASE
            let fields = gps.read_reply_raw(b"PMTK705", 2)?;
            let release = &fields[0];
            let build = &fields[1];
            info!(
                "Gps ready (firmware release {=[u8]:a}, build {=[u8]:a})",
                release, build
            );

            Ok(())
        })
        .map(|(tries, ())| {
            debug!("Now ready after {} checks", tries);
        })
        .map_err(|(tries, err)| {
            debug!("Not ready after {} checks", tries);
            err
        })
    }

    fn send_mtk_cmd<'i>(
        &mut self,
        num: &'i [u8; 3],
        fields: &'i [&'i [u8]],
    ) -> Result<(), Error<Tx::Error>> {
        debug!("Trying to send PMTK {=[u8; 3]:a} for ack", num);
        self.ensure_nmea_output_disabled()?;
        self.send_mtk_cmd_without_disabling_nmea(num, fields, MAX_CMD_TRIES)
    }

    fn send_mtk_cmd_without_disabling_nmea<'i>(
        &mut self,
        num: &'i [u8; 3],
        fields: &'i [&'i [u8]],
        max_tries: usize,
    ) -> Result<(), Error<Tx::Error>> {
        self.with_retries(max_tries, |gps| {
            let mut name = *b"PMTK\0\0\0";
            name[4..].clone_from_slice(num);

            gps.write_cmd_raw(&name, fields)?;
            gps.read_pmtk_ack_raw(num)?;

            Ok(())
        })
        .map(|(tries, ())| {
            debug!("Sent PMTK {=[u8; 3]:a} in {} tries", num, tries);
        })
        .map_err(|(tries, err)| {
            error!(
                "Failed to send PMTK {=[u8; 3]:a} after {} tries",
                num, tries
            );
            err
        })
    }

    fn send_mtk_cmd_for_reply<'i>(
        &mut self,
        num: &'i [u8; 3],
        fields: &'i [&'i [u8]],
        reply_num: &'i [u8; 3],
        reply_min_fields: usize,
    ) -> Result<Vec<Vec<u8>>, Error<Tx::Error>> {
        debug!(
            "Trying to send PMTK {=[u8; 3]:a} for reply PMTK {=[u8; 3]:a}",
            num, reply_num
        );

        self.ensure_nmea_output_disabled()?;

        self.with_retries(MAX_CMD_TRIES, |gps| {
            let mut name = *b"PMTK\0\0\0";
            name[4..].clone_from_slice(num);

            let mut reply_name = *b"PMTK\0\0\0";
            reply_name[4..].clone_from_slice(reply_num);

            gps.write_cmd_raw(&name, fields)?;
            let fields = gps.read_reply_raw(&reply_name, reply_min_fields)?;

            Ok(fields)
        })
        .map(|(tries, fields)| {
            debug!(
                "Sent PMTK {=[u8; 3]:a} for reply PMTK {=[u8; 3]:a} in {} tries",
                num, reply_num, tries
            );
            fields
        })
        .map_err(|(tries, err)| {
            error!(
                "Failed to send PMTK {=[u8; 3]:a} after {} tries",
                num, tries
            );
            err
        })
    }

    pub fn ensure_nmea_output_disabled(&mut self) -> Result<(), Error<Tx::Error>> {
        if self.disabled_nmea_output {
            debug!("Nmea output already disabled");
            return Ok(());
        }

        debug!("Disabling nmea output");
        // PMTK_API_SET_NMEA_OUTPUT
        let fields: &[&[u8]] = &[
            b"0", b"0", b"0", b"0", b"0", b"0", b"0", b"0", b"0", b"0", b"0", b"0", b"0", b"0",
            b"0", b"0", b"0", b"0", b"0",
        ];
        match self.send_mtk_cmd_without_disabling_nmea(
            b"314",
            fields,
            MAX_CMD_TRIES_WITHOUT_NMEA_DISABLED,
        ) {
            Ok(()) => {
                self.disabled_nmea_output = true;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn read_pmtk_ack_raw<'a>(&mut self, for_num: &'a [u8]) -> Result<(), Error<Tx::Error>> {
        let fields = self.read_reply_raw(b"PMTK001", 2)?;

        let got_for = &fields[0];
        let got_status = &fields[1];
        if got_status.len() != 1 {
            error!(
                "Expected PMTK_ACK status field to have one char, got: {=[u8]:a}",
                got_status
            );
            return Err(Error::Protocol);
        }
        let got_status = got_status[0];

        if for_num != got_for {
            debug!(
                "Got ack for {=[u8]:a}, expected ack for {=[u8]:a}",
                got_for, for_num
            );
            return Err(Error::Protocol);
        }

        match got_status {
            b'0' => Err(Error::GpsSaysInvalidCommand),
            b'1' => Err(Error::GpsSaysUnsupportedCommand),
            b'2' => Err(Error::GpsSaysActionFailed),
            b'3' => Ok(()),
            val => {
                error!("Unexpected PMTK_ACK flag {:a}", val);
                Err(Error::Protocol)
            }
        }
    }

    fn read_reply_raw<'a>(
        &mut self,
        name: &'a [u8],
        min_fields: usize,
    ) -> Result<Vec<Vec<u8>>, Error<Tx::Error>> {
        let (actual_name, fields) = self.read_cmd_raw()?;

        if name != actual_name {
            // This is super common if the board is sending us something else
            // and we request something at the same time. Disabling nmea output
            // helps some. Still, retrying on this is expected.
            debug!("Expected {=[u8]:a}, got {=[u8]:a}", name, actual_name);
            return Err(Error::Protocol);
        }

        if fields.len() < min_fields {
            // Failing after parse and validating command name is unexpected
            error!(
                "Expected {=[u8]:a} to have at least {} fields, got {}",
                actual_name,
                min_fields,
                fields.len()
            );
            return Err(Error::Protocol)?;
        }

        if fields.len() > min_fields {
            trace!(
                "{=[u8]:a} has {} fields, more than min_fields {}",
                actual_name,
                fields.len(),
                min_fields
            );
        }

        Ok(fields)
    }

    fn write_cmd_raw<'i>(
        &mut self,
        name: &'i [u8],
        fields: &'i [&'i [u8]],
    ) -> Result<(), Error<Tx::Error>> {
        let mut cmd = Vec::new();
        cmd::serialize(name, fields, &mut cmd);

        trace!("Sending {=[u8]:a}", &cmd);

        #[cfg(feature = "rtt-print-traffic")]
        rtt_target::rprint!(">{}", &cmd);

        let mut delayed = 0;
        for byte in cmd {
            'byte: loop {
                match self.tx.write(byte) {
                    Ok(()) => break 'byte,
                    Err(nb::Error::WouldBlock) => {
                        if delayed > MAX_WRITE_CMD_US {
                            trace!("Write timed out");
                            return Err(Error::WriteTimeout);
                        }
                        self.delay_us(1);
                        delayed += 1;
                    }
                    Err(nb::Error::Other(err)) => {
                        return Err(Error::Transmit(err));
                    }
                }
            }
        }

        trace!("Wrote (delayed {=u32:us})", delayed);

        Ok(())
    }

    pub fn flush_rx_queue(&mut self) {
        loop {
            match self.rx.split_read() {
                Ok(grant) => {
                    let len = grant.combined_len();
                    grant.release(len);
                    break;
                }
                Err(_) => continue,
            }
        }
    }

    fn read_cmd_raw(&mut self) -> Result<(Vec<u8>, Vec<Vec<u8>>), Error<Tx::Error>> {
        let mut cmd = Vec::new();
        let mut last_is_carriage_return = false;
        let mut delayed = 0;

        'outer: loop {
            if delayed > MAX_READ_CMD_US {
                trace!("Read timed out");
                return Err(Error::ReadTimeout);
            }

            // Getting a grant can fail if the queue is being written to
            let grant = match self.rx.read() {
                Ok(grant) => grant,
                Err(_) => {
                    self.delay_us(1);
                    delayed += 1;
                    continue 'outer;
                }
            };

            let mut grant_used = 0;

            for &byte in grant.buf() {
                grant_used += 1;

                if byte == b'$' && !cmd.is_empty() {
                    trace!("Resyncing");
                    cmd.clear();
                    cmd.push(byte);
                } else if byte == b'\n' && last_is_carriage_return {
                    cmd.push(byte);
                    grant.release(grant_used);
                    break 'outer;
                } else if byte == b'\r' {
                    last_is_carriage_return = true;
                    cmd.push(byte);
                } else {
                    last_is_carriage_return = false;
                    cmd.push(byte);
                }
            }

            grant.release(grant_used);
        }

        trace!("Received {=[u8]:a} (delayed {=u32:us})", &cmd, delayed);

        #[cfg(feature = "rtt-print-traffic")]
        rtt_target::rprint!("<{}", &cmd);

        cmd::parse(&cmd).map_err(Error::Parse)
    }

    fn with_retries<Op, T>(
        &mut self,
        max_tries: usize,
        mut op: Op,
    ) -> Result<(usize, T), (usize, Error<Tx::Error>)>
    where
        Op: FnMut(&mut Self) -> Result<T, Error<Tx::Error>>,
    {
        assert!(max_tries > 0);
        let mut tries = 0;
        loop {
            tries += 1;
            match op(self) {
                Ok(val) => break Ok((tries, val)),
                Err(err) if tries > max_tries => break Err((tries, err)),
                Err(_) => {
                    trace!("Delaying before retry");
                    self.delay_us(DELAY_BEFORE_RETRY_US);
                }
            }
        }
    }

    fn delay_us(&mut self, us: u32) {
        self.delay.delay_us(us);
    }
}

/// Returns a subslice of the input buffer containing the written bytes,
/// starting from the same address in memory as the input slice.
fn u32_to_base10_ascii(val: u32, out: &mut [u8; u32::FORMATTED_SIZE_DECIMAL]) -> &[u8] {
    lexical_core::write(val, out)
}

#[derive(Format, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Error<TxError> {
    /// The gps behaved in a way contrary to our understanding of the spec.
    Protocol,
    GpsSaysInvalidCommand,
    GpsSaysUnsupportedCommand,
    GpsSaysActionFailed,
    BootFailed,
    ReadTimeout,
    WriteTimeout,
    Transmit(TxError),
    Parse(ParseError),
    ParseLoggedPoint(ParseLoggedPointError),
}

impl<TxError> From<ParseError> for Error<TxError> {
    fn from(err: ParseError) -> Self {
        Self::Parse(err)
    }
}

impl<TxError> From<ParseLoggedPointError> for Error<TxError> {
    fn from(err: ParseLoggedPointError) -> Self {
        Self::ParseLoggedPoint(err)
    }
}

#[cfg(all(test, not(target_os = "none")))]
mod tests {
    use super::*;

    #[test]
    fn test_configure_logger_interval() {
        let expects = [
            MockTrans::write_many(b"$PMTK187,1,5*38\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK001,187,3*3E\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.configure_logger_interval(5).unwrap();

        mock.done();
    }

    #[test]
    fn test_erase_logs() {
        let expects = [
            MockTrans::write_many(b"$PMTK184,1*22\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK001,184,3*3D\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.erase_logs().unwrap();

        mock.done();
    }

    #[test]
    fn test_start_logging() {
        let expects = [
            MockTrans::write_many(b"$PMTK185,0*22\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK001,185,3*3C\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.start_logging().unwrap();

        mock.done();
    }

    #[test]
    fn test_stop_logging() {
        let expects = [
            MockTrans::write_many(b"$PMTK185,1*23\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK001,185,3*3C\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.stop_logging().unwrap();

        mock.done();
    }

    #[test]
    fn test_logger_status() {
        let expects = [
            MockTrans::write_many(b"$PMTK183*38\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTKLOG,456,0,11,31,2,0,0,0,3769,46*48\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        let actual = gps.logger_status().unwrap();
        let expected = LoggerStatus {
            interval: 2,
            is_on: true,
            record_count: 3769,
            percent_full: IntegerPercent::new(46),
        };
        assert_eq!(actual, expected);

        mock.done();
    }

    #[test]
    fn test_read_logs() {
        todo!()
    }

    #[test]
    fn test_hot_restart() {
        let expects = [
            // Factory reset
            MockTrans::write_many(b"$PMTK101*32\r\n"),
            MockTrans::flush(),
            // Boot messages
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            MockTrans::read_many(b"$CDACK,105*56\r\n"),
            MockTrans::read_many(b"$PMTK011,MTKGPS*08\r\n"),
            MockTrans::read_many(b"$PMTK010,002*2D\r\n"),
            MockTrans::read_many(b"$PMTK010,001*2E\r\n"),
            // Get firmware version
            MockTrans::write_many(b"$PMTK605*31\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK705,AXN_1.3,2102,ABCD,*11\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.hot_restart().unwrap();

        mock.done();
    }

    #[test]
    fn test_warn_restart() {
        let expects = [
            // Factory reset
            MockTrans::write_many(b"$PMTK102*31\r\n"),
            MockTrans::flush(),
            // Boot messages
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            MockTrans::read_many(b"$CDACK,105*56\r\n"),
            MockTrans::read_many(b"$PMTK011,MTKGPS*08\r\n"),
            MockTrans::read_many(b"$PMTK010,002*2D\r\n"),
            MockTrans::read_many(b"$PMTK010,001*2E\r\n"),
            // Get firmware version
            MockTrans::write_many(b"$PMTK605*31\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK705,AXN_1.3,2102,ABCD,*11\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.warm_restart().unwrap();

        mock.done();
    }

    #[test]
    fn test_cold_restart() {
        let expects = [
            // Factory reset
            MockTrans::write_many(b"$PMTK103*30\r\n"),
            MockTrans::flush(),
            // Boot messages
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            MockTrans::read_many(b"$CDACK,105*56\r\n"),
            MockTrans::read_many(b"$PMTK011,MTKGPS*08\r\n"),
            MockTrans::read_many(b"$PMTK010,002*2D\r\n"),
            MockTrans::read_many(b"$PMTK010,001*2E\r\n"),
            // Get firmware version
            MockTrans::write_many(b"$PMTK605*31\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK705,AXN_1.3,2102,ABCD,*11\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.cold_restart().unwrap();

        mock.done();
    }

    #[test]
    fn test_factory_reset() {
        let expects = [
            // Factory reset
            MockTrans::write_many(b"$PMTK104*37\r\n"),
            MockTrans::flush(),
            // Boot messages
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            MockTrans::read_many(b"$CDACK,105*56\r\n"),
            MockTrans::read_many(b"$PMTK011,MTKGPS*08\r\n"),
            MockTrans::read_many(b"$PMTK010,002*2D\r\n"),
            MockTrans::read_many(b"$PMTK010,001*2E\r\n"),
            // Get firmware version
            MockTrans::write_many(b"$PMTK605*31\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK705,AXN_1.3,2102,ABCD,*11\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.factory_reset().unwrap();

        mock.done();
    }

    #[test]
    fn test_send_reboot_cmd_when_first_try_fails() {
        let expects = [
            // Try 1
            MockTrans::write_many(b"$PMTK104*37\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            MockTrans::read_many(b"$CDACK,105*56\r\n"),
            MockTrans::read_many(b"$PMTK010,001*2E\r\n"),
            MockTrans::read_many(b"$PMTK010,002*2D\r\n"),
            // Missing PMTK011,MTKGPS
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            // Try 2
            MockTrans::write_many(b"$PMTK104*37\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            MockTrans::read_many(b"$CDACK,105*56\r\n"),
            MockTrans::read_many(b"$PMTK011,MTKGPS*08\r\n"),
            MockTrans::read_many(b"$PMTK010,002*2D\r\n"),
            MockTrans::read_many(b"$PMTK010,001*2E\r\n"),
            // Get firmware version
            MockTrans::write_many(b"$PMTK605*31\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK705,AXN_1.3,2102,ABCD,*11\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.send_reboot_cmd(b"PMTK104").unwrap();

        mock.done();
    }

    #[test]
    fn test_send_reboot_cmd_when_missing_mtkgps() {
        let expects = [
            // Try 1
            MockTrans::write_many(b"$PMTK103*30\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            MockTrans::read_many(b"$CDACK,105*56\r\n"),
            MockTrans::read_many(b"$PMTK010,001*2E\r\n"),
            MockTrans::read_many(b"$PMTK010,002*2D\r\n"),
            // Missing PMTK011,MTKGPS
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            // Try 2
            MockTrans::write_many(b"$PMTK103*30\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            MockTrans::read_many(b"$CDACK,105*56\r\n"),
            MockTrans::read_many(b"$PMTK011,MTKGPS*08\r\n"),
            MockTrans::read_many(b"$PMTK010,002*2D\r\n"),
            MockTrans::read_many(b"$PMTK010,001*2E\r\n"),
            // Get firmware version
            MockTrans::write_many(b"$PMTK605*31\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK705,AXN_1.3,2102,ABCD,*11\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.send_reboot_cmd(b"PMTK103").unwrap();

        mock.done();
    }

    #[test]
    fn test_send_reboot_cmd_retries_when_missing_boot_sys_msg() {
        let expects = [
            MockTrans::write_many(b"$PMTK104*37\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            MockTrans::read_many(b"$CDACK,105*56\r\n"),
            // Missing PMTK010,001
            MockTrans::read_many(b"$PMTK010,002*2D\r\n"),
            MockTrans::read_many(b"$PMTK011,MTKGPS*08\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            // Try 2
            MockTrans::write_many(b"$PMTK104*37\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            MockTrans::read_many(b"$CDACK,105*56\r\n"),
            MockTrans::read_many(b"$PMTK011,MTKGPS*08\r\n"),
            MockTrans::read_many(b"$PMTK010,002*2D\r\n"),
            MockTrans::read_many(b"$PMTK010,001*2E\r\n"),
            // Get firmware version
            MockTrans::write_many(b"$PMTK605*31\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK705,AXN_1.3,2102,ABCD,*11\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.send_reboot_cmd(b"PMTK104").unwrap();

        mock.done();
    }

    #[test]
    fn test_send_reboot_cmd_retries_when_reading_firmware_fails() {
        let expects = [
            // Send PMTK_CMD_FULL_COLD_START
            MockTrans::write_many(b"$PMTK104*37\r\n"),
            MockTrans::flush(),
            // Boot messages
            MockTrans::read_many(b"$CDACK,34,0*79\r\n"),
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            MockTrans::read_many(b"$CDACK,105*56\r\n"),
            MockTrans::read_many(b"$PMTK011,MTKGPS*08\r\n"),
            MockTrans::read_many(b"$PMTK010,001*2E\r\n"),
            // Get firmware version try 1
            MockTrans::write_many(b"$PMTK605*31\r\n"),
            MockTrans::flush(),
            // Spurious response
            MockTrans::read_many(b"$CDACK,103*50\r\n"),
            // Get firmware version try 2
            MockTrans::write_many(b"$PMTK605*31\r\n"),
            MockTrans::flush(),
            // Firmware version response
            MockTrans::read_many(b"$PMTK705,AXN_1.3,2102,ABCD,*11\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.send_reboot_cmd(b"PMTK104").unwrap();

        mock.done();
    }

    #[test]
    fn nmea_disabled_on_first_cmd_only() {
        let expects = [
            // Disable nmea output
            MockTrans::write_many(b"$PMTK314,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0*28\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK001,314,3*36\r\n"),
            // Erase logs
            MockTrans::write_many(b"$PMTK184,1*22\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK001,184,3*3D\r\n"),
            // Erase logs
            MockTrans::write_many(b"$PMTK184,1*22\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK001,184,3*3D\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), false);

        gps.erase_logs().unwrap();
        gps.erase_logs().unwrap();

        mock.done();
    }

    #[test]
    fn test_send_pmtk_cmd() {
        let expects = [
            MockTrans::write_many(b"$PMTK187,10,5*08\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK001,187,3*3E\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.send_mtk_cmd(b"187", &[b"10", b"5"]).unwrap();

        mock.done();
    }

    #[test]
    fn test_send_pmtk_cmd_retries() {
        let expects = [
            // Try 1
            MockTrans::write_many(b"$PMTK187,10,5*08\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"foo\r\n"),
            // Try 2
            MockTrans::write_many(b"$PMTK187,10,5*08\r\n"),
            MockTrans::flush(),
            MockTrans::read_many(b"$PMTK001,187,3*3E\r\n"),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.send_mtk_cmd(b"187", &[b"10", b"5"]).unwrap();

        mock.done();
    }

    #[test]
    fn test_read_pmtk_ack_raw_when_not_ack() {
        let expects = [MockTrans::read_many(b"$PMTK002*30\r\n")];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        let actual = gps.read_pmtk_ack_raw(b"604");
        assert_eq!(actual, Err(Error::Protocol));

        mock.done();
    }

    #[test]
    fn test_read_pmtk_ack_raw_when_wrong_fields() {
        let expects = [MockTrans::read_many(b"$PMTK001,600*29\r\n")];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        let actual = gps.read_pmtk_ack_raw(b"604");
        assert_eq!(actual, Err(Error::Protocol));

        mock.done();
    }

    #[test]
    fn test_read_pmtk_ack_raw_when_for_incorrect() {
        let expects = [MockTrans::read_many(b"$PMTK001,600,3*36\r\n")];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        let actual = gps.read_pmtk_ack_raw(b"604");
        assert_eq!(actual, Err(Error::Protocol));

        mock.done();
    }

    #[test]
    fn test_read_pmtk_ack_raw_when_gps_says_invalid() {
        let expects = [MockTrans::read_many(b"$PMTK001,600,0*35\r\n")];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        let actual = gps.read_pmtk_ack_raw(b"600");
        assert_eq!(actual, Err(Error::GpsSaysInvalidCommand));

        mock.done();
    }

    #[test]
    fn test_read_pmtk_ack_raw_when_gps_says_unsupported() {
        let expects = [MockTrans::read_many(b"$PMTK001,600,1*34\r\n")];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        let actual = gps.read_pmtk_ack_raw(b"600");
        assert_eq!(actual, Err(Error::GpsSaysUnsupportedCommand));

        mock.done();
    }

    #[test]
    fn test_read_pmtk_ack_raw_when_gps_says_failed() {
        let expects = [MockTrans::read_many(b"$PMTK001,600,2*37\r\n")];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        let actual = gps.read_pmtk_ack_raw(b"600");
        assert_eq!(actual, Err(Error::GpsSaysActionFailed));

        mock.done();
    }

    #[test]
    fn test_read_pmtk_ack_raw_for_correct() {
        let expects = [MockTrans::read_many(b"$PMTK001,604,3*32\r\n")];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        gps.read_pmtk_ack_raw(b"604").unwrap();

        mock.done();
    }

    #[test]
    fn test_write_cmd_raw() {
        let expects = [
            MockTrans::write_many(b"$PMTK187,10,5*08\r\n"),
            MockTrans::flush(),
        ];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        let fields: Vec<&[u8]> = vec![b"10", b"5"];
        gps.write_cmd_raw(b"PMTK187", &fields).unwrap();

        mock.done();
    }

    #[test]
    fn test_read_cmd_raw() {
        let expects = [MockTrans::read_many(b"$PMTK187,10,5*08\r\n")];
        let mut mock = MockSerial::new(&expects);
        let mut gps = Gps::new(mock.clone(), mock.clone(), NoopDelay::new(), true);

        let (actual_name, actual_fields) = gps.read_cmd_raw().unwrap();
        let expected_name = b"PMTK187";
        let expected_fields: Vec<&[u8]> = vec![b"10", b"5"];
        assert_eq!(actual_name, expected_name);
        assert_eq!(actual_fields, expected_fields);

        mock.done();
    }
}
