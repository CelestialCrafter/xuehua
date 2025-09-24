package main

import (
	"bytes"
	"encoding/json/jsontext"
	"encoding/json/v2"
	"errors"
	"fmt"
	"os"
	"os/exec"
)

type CommandRequest struct {
	Program     string   `json:"program"`
	Args        []string `json:"args"`
	WorkingDir  string   `json:"working_dir"`
	Environment []string `json:"environment"`
}

type CommandResponseInfo struct {
	ExitCode int    `json:"exit_code"`
	Stdout   []byte `json:"stdout,format:array"`
	Stderr   []byte `json:"stderr,format:array"`
}

type CommandResponse struct {
	Error *string              `json:"error,omitempty"`
	Info  *CommandResponseInfo `json:"info,omitempty"`
}

func handleLine(decoder *jsontext.Decoder) (*CommandResponseInfo, error) {
	var req CommandRequest
	err := json.UnmarshalDecode(decoder, &req)
	if err != nil {
		return nil, fmt.Errorf("error deserializing request: %w", err)
	}

	cmd := exec.Command(req.Program, req.Args...)
	cmd.Env = append(cmd.Env, req.Environment...)
	cmd.Dir = req.WorkingDir

	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	err = cmd.Run()
	exitCode := 0
	stdoutBytes := stdout.Bytes()
	stderrBytes := stderr.Bytes()

	if err != nil {
		var exitError *exec.ExitError
		if errors.As(err, &exitError) {
			exitCode = exitError.ExitCode()
		} else {
			return nil, fmt.Errorf("command failed: %s", err)
		}
	}

	return &CommandResponseInfo{
		ExitCode: exitCode,
		Stdout:   stdoutBytes,
		Stderr:   stderrBytes,
	}, nil
}

func main() {
	decoder := jsontext.NewDecoder(os.Stdin)
	encoder := jsontext.NewEncoder(os.Stdout)

	for {
		resp := CommandResponse{}

		info, err := handleLine(decoder)
		if err != nil {
			decoder.Reset(os.Stdin)
			errString := err.Error()
			resp.Error = &errString
		} else {
			resp.Info = info
		}

		err = json.MarshalEncode(encoder, resp)
		if err != nil {
			panic(fmt.Sprint("could not encode response", err))
		}
	}
}
