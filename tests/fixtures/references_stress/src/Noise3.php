<?php

namespace App;

class Noise3
{
    private function compute(): int
    {
        return 3;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
