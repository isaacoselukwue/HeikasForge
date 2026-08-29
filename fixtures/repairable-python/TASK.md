Round invoice currency half away from zero

The invoice module currently rounds with banker's rounding, so a line total of 1.005 becomes 1.00 instead of 1.01. Change the rounding policy so that a half value always rounds away from zero, keep the public function names, and make sure the existing test suite passes without altering it.
