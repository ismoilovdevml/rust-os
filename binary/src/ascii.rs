use std::io::{self, Write};

fn main() {
    println!("Matn kiriting:");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim();

    let mut total_ascii = 0;
    let mut total_size = 0;

    for c in input.chars() {
        let ascii_val = c as u32;
        let bin_val = format!("{:b}", ascii_val);
        let size = bin_val.len();
        total_ascii += ascii_val;
        total_size += size;

        println!("Belgi: '{}', ASCII: {}, ikkilik: {}, o'lchami: {}",
                 c, ascii_val, bin_val, size);
    }

    let total_bin_val = format!("{:b}", total_ascii);
    println!("umumiy ASCII: {}, umumiy ikkilikda: {}, umumiy o'lchami: {}",
             total_ascii, total_bin_val, total_size);
}
