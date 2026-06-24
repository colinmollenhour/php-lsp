<?php

namespace App;

class Noise12
{
    private function compute(): int
    {
        return 12;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
