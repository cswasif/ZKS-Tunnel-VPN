// Package debug provides a global debug flag for toggling verbose logging
package debug

import "fmt"

// Enabled controls whether debug output is printed
var Enabled = false

// Printf prints debug output if debug mode is enabled
func Printf(format string, args ...interface{}) {
	if Enabled {
		fmt.Printf("[DEBUG] "+format, args...)
	}
}

// Println prints debug output if debug mode is enabled
func Println(args ...interface{}) {
	if Enabled {
		allArgs := append([]interface{}{"[DEBUG]"}, args...)
		fmt.Println(allArgs...)
	}
}
