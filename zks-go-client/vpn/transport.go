package vpn

import (
	"fmt"
	"net"

	"github.com/zks-vpn/zks-go-client/protocol"
	"github.com/zks-vpn/zks-go-client/relay"
)

// Transport defines the interface for sending/receiving VPN packets
type Transport interface {
	// SendBatch sends a batch of IP packets
	SendBatch(packets [][]byte) error
	// Recv receives a message (IpPacket or BatchIpPacket)
	Recv() (protocol.TunnelMessage, error)
	// Close closes the transport
	Close()
}

// RelayTransport wraps the WebSocket relay connection
type RelayTransport struct {
	conn *relay.Connection
}

// NewRelayTransport creates a new RelayTransport
func NewRelayTransport(conn *relay.Connection) *RelayTransport {
	return &RelayTransport{conn: conn}
}

func (t *RelayTransport) SendBatch(packets [][]byte) error {
	if len(packets) == 0 {
		return nil
	}
	// Wrap in BatchIpPacket
	msg := &protocol.BatchIpPacket{Packets: packets}
	return t.conn.Send(msg)
}

func (t *RelayTransport) Recv() (protocol.TunnelMessage, error) {
	return t.conn.Recv()
}

func (t *RelayTransport) Close() {
	t.conn.Close()
}

// UDPTransport implements direct UDP connection to Entry Node
// Note: This sends RAW IP packets over UDP (no encryption layer yet)
// Security relies on inner TLS/HTTPS of the traffic itself.
type UDPTransport struct {
	conn *net.UDPConn
}

// NewUDPTransport creates a new UDPTransport connected to the Entry Node
func NewUDPTransport(addr string) (*UDPTransport, error) {
	udpAddr, err := net.ResolveUDPAddr("udp", addr)
	if err != nil {
		return nil, fmt.Errorf("resolve failed: %w", err)
	}
	
	// Connect to the Entry Node
	conn, err := net.DialUDP("udp", nil, udpAddr)
	if err != nil {
		return nil, fmt.Errorf("dial failed: %w", err)
	}
	
	return &UDPTransport{conn: conn}, nil
}

func (t *UDPTransport) SendBatch(packets [][]byte) error {
	for _, pkt := range packets {
		// Send each packet individually over UDP
		// TODO: Use sendmmsg for optimization on supported platforms
		_, err := t.conn.Write(pkt)
		if err != nil {
			return err
		}
	}
	return nil
}

func (t *UDPTransport) Recv() (protocol.TunnelMessage, error) {
	// Buffer for receiving UDP packet
	// Max UDP size is 65535, but MTU is usually 1500. Safe to use larger buffer.
	buf := make([]byte, 65535)
	
	n, _, err := t.conn.ReadFromUDP(buf)
	if err != nil {
		return nil, err
	}
	
	// Copy data to a new slice to fit exact size
	// (StartTUN expects to own the data)
	payload := make([]byte, n)
	copy(payload, buf[:n])
	
	// Wrap in IpPacket for compatibility with StartTUN logic
	return &protocol.IpPacket{Payload: payload}, nil
}

func (t *UDPTransport) Close() {
	t.conn.Close()
}
