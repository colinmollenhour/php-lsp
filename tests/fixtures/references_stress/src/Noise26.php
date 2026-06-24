<?php

namespace App;

class Noise26
{
    private function compute(): int
    {
        return 26;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
