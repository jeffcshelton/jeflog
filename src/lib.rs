use std::{
	io::{self, Write},
	sync::{atomic::{AtomicBool, Ordering}, Mutex}, thread, time::Duration,
};

#[derive(Clone, Copy, Debug)]
struct Task {
	pub row_offset: i32
}

static TASKS: Mutex<Vec<Task>> = Mutex::new(Vec::new());
static SPINNING: AtomicBool = AtomicBool::new(false);

/// Begins a task or subtask with a spinner.
#[macro_export]
macro_rules! task {
	($($tokens:tt)*) => {
		$crate::__start_task__(format!($($tokens)*));
	}
}

/// Indicates that the most recently created task has passed by
/// replacing the spinner with a green check mark.
#[macro_export]
macro_rules! pass {
	($($tokens:tt)*) => {
		$crate::__end_task__("\x1b[32;1m✔\x1b[0m", format!($($tokens)*));
	}
}

/// Indicates that the most recently created task has passed with a
/// warning by replacing the spinner with a yellow triangle.
#[macro_export]
macro_rules! warn {
	($($tokens:tt)*) => {
		$crate::__end_task__("\x1b[33;1m▲\x1b[0m", format!($($tokens)*));
	}
}

/// Indicates that the most recently created task has failed by
/// replacing the spinner with a red x.
#[macro_export]
macro_rules! fail {
	($($tokens:tt)*) => {
		$crate::__end_task__("\x1b[31;1m✘\x1b[0m", format!($($tokens)*));
	}
}

#[doc(hidden)]
pub fn __start_task__(message: String) {
	// this can never panic because mutex locks can only
	// fail if the thread holding the lock panics.
	// this is guaranteed as long as:
	//   1. TASKS is never locked outside of jeflog
	//   2. jeflog code never panics
	// as long as these two invariants are satisfied
	// (and they are by design) then locks of TASKS
	// cannot panic.
	let mut tasks = TASKS.lock().unwrap();

	// adjust the offset (from bottom row) of each task
	for task in tasks.iter_mut() {
		task.row_offset += 1;
	}

	println!();

	if let Some(last_row) = tasks.last().map(|task| task.row_offset) {
		print!("\x1b[s");

		if last_row > 1 {
			print!("\x1b[{}A\x1b[{}G┣", last_row - 1, (tasks.len() - 1) * 5 + 3);
		}

		for _ in 1..last_row {
			print!("\x1b[1D\x1b[1B┃");
		}

		print!("\x1b[u");
	}

	tasks.push(Task { row_offset: 0 });

	if tasks.len() > 1 {
		print!("{}", " ".repeat((tasks.len() - 2) * 5 + 2) + "┗━ ");
	}

	// attempt to print message, ignore if flush fails
	print!("\x1b[33;1m-\x1b[0m {message}");
	_ = io::stdout().flush();

	// atomically check if the spinner is running
	// if not, then start the spinner
	if SPINNING.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed) == Ok(false) {
		thread::spawn(spin);
	}
}

#[doc(hidden)]
pub fn __end_task__(symbol: &str, message: String) {
	let mut tasks = TASKS.lock().unwrap();

	if let Some(Task { row_offset: row }) = tasks.pop() {
		let column = tasks.len() * 5 + 1;
		// replace spinner with symbol:
		// \x1b[s         : save cursor's current position
		// \x1b[{row}A    : move the cursor up to correct row
		// \x1b[{column}G : move the cursor to correct column
		// {symbol}       : print the symbol replacing the spinner
		// \x1b[K         : clear the current line
		// {message}      : print the ending message overwriting the old message

		print!("\x1b[s");

		if row > 0 {
			print!("\x1b[{row}A");
		}

		print!("\x1b[{column}G{symbol} \x1b[K{message}");

		// restore the cursor's position if not the last task
		if row != 0 {
			print!("\x1b[u");
		}

		_ = io::stdout().flush();
	} else {
		// if no task is running, just print the symbol and message
		println!("{symbol} {message}");
	}

	if tasks.len() == 0 {
		println!();
	}
}

fn spin() {
	let mut spinner = '-';

	loop {
		let tasks = TASKS.lock().unwrap();

		// kill the thread if there are no more tasks
		if tasks.len() == 0 {
			break;
		}

		let mut column = 1;

		for Task { row_offset: row } in tasks.iter() {
			// replace spinner with new spinner:
			// \x1b[s         : save the cursor's current position
			// \x1b[{row}A    : move the cursor up to correct row
			// \x1b[{column}G : move the cursor to correct column
			// \x1b[33;1m     : set the foreground color to yellow and font to bold
			// {spinner}      : print the updated spinner character
			// \x1b[0m        : reset all formatting
			// \x1b[u         : restore saved cursor position

			print!("\x1b[s");

			if *row > 0 {
				print!("\x1b[{row}A");
			}

			print!("\x1b[{column}G\x1b[33;1m{spinner}\x1b[0m\x1b[u");
			
			column += 5;
		}

		// most systems flush stdout by newlines
		// since no newlines were printed, we need
		// to flush stdout explicitly
		_ = io::stdout().flush();

		// update spinner to next spinner character (clockwise)
		spinner = match spinner {
			'-' => '\\',
			'\\' => '|',
			'|' => '/',
			'/' => '-',
			_ => '-', // this is not possible, but Rust demands it
		};

		// drop tasks before the wait so other threads may use it
		drop(tasks);

		// wait for 100ms; this can be changed to make the spinner go faster
		thread::sleep(Duration::from_millis(100));
	}

	// if the loop has ended, then the spinner has stopped and
	// will need to be restarted if another task starts
	SPINNING.store(false, Ordering::Relaxed);
}
