<?php
namespace Lib\Transport;

class SmtpClient {
    public function deliver(string $to, string $subject, string $body): bool {
        return true;
    }
}
