#[allow(dead_code)]
mod model {
    include!("reference/rtc-model.rs");
    pub fn proof() {
        let file = file_hash(17);
        assert_eq!(file, 695);
        let rtc = encode(1, file, 0);
        assert_eq!(
            rtc,
            Rtc {
                year: 2021,
                month: 4,
                day: 10,
                hour: 0,
                minute: 0,
                second: 0
            }
        );
        println!("action=17\nname=console_resume_all\nvalue=1\nstart=false\nuser=1\nfile_hash={file}\ndevice_hash=0\nencoded_rtc={rtc:?}\nactual_hardware_readback=pending\n");
    }
}
fn main() {
    model::proof();
}
