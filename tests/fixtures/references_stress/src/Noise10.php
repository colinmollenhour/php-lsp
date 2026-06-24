<?php

namespace App;

class Noise10
{
    private function compute(): int
    {
        return 10;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
