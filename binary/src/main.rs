use std::io;

fn main() {
    println!("O'nlik sanoq tizimida son yozing");

    let mut input = String::new();
    io::stdin().read_line(&mut input)
        .expect("O'qib bo'lmadi");

    let input = input.trim().parse::<i32>()
        .expect("Iltimos faqat raqamlar yozing");

    let binary = format!("{:b}", input);

    println!("Ikkilikdagi ko'rinishi: {}", binary);
}
