package vpn

import (
	"fmt"
	"net"
	"os/exec"
	"strings"

	"github.com/zks-vpn/zks-go-client/debug"
	"github.com/zks-vpn/zks-go-client/protocol"
	"github.com/zks-vpn/zks-go-client/relay"
	"golang.zx2c4.com/wireguard/tun"
)

// TUN represents a TUN device interface
type TUN struct {
	device tun.Device
	conn   *relay.Connection
	name   string
	ip     string
}

// NewTUN creates a new TUN device
func NewTUN(conn *relay.Connection) (*TUN, error) {
	// Create TUN device using Wintun
	// Name: "ZKS-VPN"
	// We use the wireguard-go/tun package which handles Wintun driver loading
	dev, err := tun.CreateTUN("ZKS-VPN", 0)
	if err != nil {
		return nil, fmt.Errorf("failed to create TUN device: %w", err)
	}

	name, err := dev.Name()
	if err != nil {
		dev.Close()
		return nil, fmt.Errorf("failed to get device name: %w", err)
	}
	debug.Printf("Created TUN device: %s\n", name)

	return &TUN{
		device: dev,
		conn:   conn,
		name:   name,
		ip:     "10.0.85.1", // Default client IP
	}, nil
}

// Start starts the packet processing loops
func (t *TUN) Start() error {
	// Configure IP address and routes
	if err := t.configureInterface(); err != nil {
		t.device.Close()
		return fmt.Errorf("failed to configure interface: %w", err)
	}

	// Start read loop (TUN -> Relay)
	go t.readLoop()

	// Start write loop (Relay -> TUN)
	go t.writeLoop()

	return nil
}

// Stop closes the TUN device
func (t *TUN) Stop() {
	if t.device != nil {
		t.device.Close()
	}
}

// configureInterface sets IP and routes using netsh
func (t *TUN) configureInterface() error {
	debug.Println("Configuring TUN interface...")

	// 1. Set IP address
	// netsh interface ip set address "ZKS-VPN" static 10.0.85.1 255.255.255.0
	cmd := exec.Command("netsh", "interface", "ip", "set", "address", t.name, "static", t.ip, "255.255.255.0")
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("failed to set IP: %v, output: %s", err, out)
	}

	// 2. Add route to Relay IP via default gateway (to prevent loop)
	// We need to find the default gateway first
	gateway, err := getDefaultGateway()
	if err != nil {
		return fmt.Errorf("failed to get default gateway: %w", err)
	}
	
	// Resolve relay hostname to IP
	relayHost := "zks-tunnel-relay.md-wasif-faisal.workers.dev"
	relayIPs, err := net.LookupIP(relayHost)
	if err != nil {
		return fmt.Errorf("failed to resolve relay host: %w", err)
	}
	
	for _, ip := range relayIPs {
		if ip.To4() != nil {
			debug.Printf("Adding route for relay IP %s via gateway %s\n", ip, gateway)
			// route add <relay_ip> mask 255.255.255.255 <gateway>
			cmd = exec.Command("route", "add", ip.String(), "mask", "255.255.255.255", gateway)
			if out, err := cmd.CombinedOutput(); err != nil {
				debug.Printf("Warning: failed to add route for relay: %v, output: %s\n", err, out)
			}
		}
	}

	// 3. Add default route override via TUN (def1 trick)
	// Instead of replacing 0.0.0.0/0, we add 0.0.0.0/1 and 128.0.0.0/1
	// This is more specific than 0.0.0.0/0 so it takes precedence
	debug.Println("Adding default route override (0.0.0.0/1 and 128.0.0.0/1)...")
	
	// route add 0.0.0.0 mask 128.0.0.0 10.0.85.1 metric 1
	cmd = exec.Command("route", "add", "0.0.0.0", "mask", "128.0.0.0", t.ip, "metric", "1")
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("failed to add 0.0.0.0/1 route: %v, output: %s", err, out)
	}

	// route add 128.0.0.0 mask 128.0.0.0 10.0.85.1 metric 1
	cmd = exec.Command("route", "add", "128.0.0.0", "mask", "128.0.0.0", t.ip, "metric", "1")
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("failed to add 128.0.0.0/1 route: %v, output: %s", err, out)
	}

	return nil
}

// readLoop reads packets from TUN and sends to Relay
func (t *TUN) readLoop() {
	debug.Println("TUN: Starting read loop...")
	// wireguard-go/tun Read() takes [][]byte (scatter/gather)
	// We just use a single buffer
	buf := make([]byte, 2048) // MTU is usually 1500
	buffs := [][]byte{buf}
	sizes := []int{0}

	for {
		n, err := t.device.Read(buffs, sizes, 0)
		if err != nil {
			debug.Printf("TUN: Read error: %v\n", err)
			break
		}

		if n == 0 || sizes[0] == 0 {
			continue
		}
		size := sizes[0]

		// Skip non-IPv4 packets (simple check)
		if size < 20 {
			continue
		}
		version := (buf[0] >> 4) & 0x0F
		if version != 4 {
			continue
		}

		debug.Printf("TUN: Read IPv4 packet, len=%d bytes\n", size)

		// Create IpPacket message
		payload := make([]byte, size)
		copy(payload, buf[:size])
		msg := &protocol.IpPacket{
			Payload: payload,
		}

		// Send to Relay
		debug.Printf("TUN: Sending packet to relay (len=%d)\n", size)
		if err := t.conn.Send(msg); err != nil {
			debug.Printf("TUN: Failed to send packet to relay: %v\n", err)
			break
		}
	}
	debug.Println("TUN: Read loop exited")
}

// writeLoop receives packets from Relay and writes to TUN
func (t *TUN) writeLoop() {
	debug.Println("TUN: Starting write loop...")
	for {
		msg, err := t.conn.Recv()
		if err != nil {
			debug.Printf("TUN: Relay receive error: %v\n", err)
			break
		}

		if packet, ok := msg.(*protocol.IpPacket); ok {
			debug.Printf("TUN: Received packet from relay (len=%d)\n", len(packet.Payload))
			// wireguard-go/tun Write() takes [][]byte
			buffs := [][]byte{packet.Payload}
			if _, err := t.device.Write(buffs, 0); err != nil {
				debug.Printf("TUN: Write error: %v\n", err)
			} else {
				debug.Printf("TUN: Wrote packet to interface (len=%d)\n", len(packet.Payload))
			}
		} else {
			debug.Printf("TUN: Ignoring non-IP packet type: %T\n", msg)
		}
	}
	debug.Println("TUN: Write loop exited")
}

// Helper to get default gateway (simplified implementation)
func getDefaultGateway() (string, error) {
	// Run "route print 0.0.0.0" and parse output
	cmd := exec.Command("route", "print", "0.0.0.0")
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}

	lines := strings.Split(string(out), "\n")
	for _, line := range lines {
		fields := strings.Fields(line)
		if len(fields) > 2 && fields[0] == "0.0.0.0" {
			return fields[2], nil
		}
	}
	return "", fmt.Errorf("default gateway not found")
}
